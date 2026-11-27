//! Authenticated two-peer TCP/TLS runtime with receiver-observed heartbeats.
//!
//! Each peer maintains one outbound TLS connection and accepts the peer's
//! outbound TLS connection. The two TCP connections are independent, so a
//! one-direction packet blackhole does not automatically suppress evidence
//! travelling in the surviving direction. TLS authenticates both peers;
//! configured SHA-256 leaf-certificate pins bind the session to exact peers.

use crate::{
    decode_frame, encode_frame, BidirectionalDelivery, Direction, DirectionState, MessageKind,
    ReceiptPolicy, SessionState, HEADER_LEN, MAX_PAYLOAD_LEN,
};
use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    time,
};
use tokio_rustls::{
    rustls::{
        pki_types::{CertificateDer, PrivateKeyDer, ServerName},
        server::WebPkiClientVerifier,
        ClientConfig, RootCertStore, ServerConfig,
    },
    TlsAcceptor, TlsConnector,
};

const MAX_QUEUED_REPORTS: usize = 2048;
const MAX_REPORT_GAP: u64 = 4096;
const REPORT_RECEIVED: u8 = 0;
const REPORT_MISSING: u8 = 1;

#[derive(Debug, Clone)]
pub struct PeerConfig {
    pub local_is_a: bool,
    pub bind_addr: SocketAddr,
    pub peer_addr: SocketAddr,
    pub peer_id: String,
    pub ca_certificate_der: Vec<u8>,
    pub local_certificate_der: Vec<u8>,
    pub local_private_key_der: Vec<u8>,
    pub expected_peer_sha256: [u8; 32],
    pub session_id: [u8; 16],
    pub probe_interval: Duration,
    pub observation_timeout: Duration,
    pub receipt_policy: ReceiptPolicy,
}

pub async fn run_peer(config: PeerConfig) -> Result<()> {
    if config.probe_interval.is_zero() || config.observation_timeout <= config.probe_interval {
        bail!("observation timeout must exceed a non-zero probe interval");
    }

    let (acceptor, connector) = build_tls(&config)?;
    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("bind {}", config.bind_addr))?;
    let (report_tx, report_rx) = mpsc::channel::<Vec<u8>>(MAX_QUEUED_REPORTS);
    let delivery = Arc::new(Mutex::new(BidirectionalDelivery::new(
        config.receipt_policy,
    )?));
    let config = Arc::new(config);
    let monitor_delivery = Arc::clone(&delivery);
    let monitor_role = config.local_is_a;
    tokio::spawn(async move {
        let mut ticker = time::interval(Duration::from_secs(1));
        loop {
            ticker.tick().await;
            if let Ok(state) = monitor_delivery.lock() {
                let (session, a_to_b, b_to_a) = session_snapshot(&state);
                println!(
                    "ARES_STATE local={} session={:?} a_to_b={:?} b_to_a={:?}",
                    local_name(monitor_role),
                    session,
                    a_to_b,
                    b_to_a
                );
            }
        }
    });

    let accept_task = accept_loop(
        listener,
        acceptor,
        report_tx,
        Arc::clone(&delivery),
        Arc::clone(&config),
    );
    let connect_task = connect_loop(
        connector,
        report_rx,
        Arc::clone(&delivery),
        Arc::clone(&config),
    );
    tokio::select! {
        result = accept_task => result,
        result = connect_task => result,
    }
}

fn build_tls(config: &PeerConfig) -> Result<(TlsAcceptor, TlsConnector)> {
    let ca = CertificateDer::from(config.ca_certificate_der.clone());
    let certificate = CertificateDer::from(config.local_certificate_der.clone());
    let key = PrivateKeyDer::try_from(config.local_private_key_der.clone())
        .map_err(|error| anyhow!("invalid local private key: {error}"))?;

    let mut roots = RootCertStore::empty();
    roots.add(ca)?;
    let roots = Arc::new(roots);

    let client_verifier = WebPkiClientVerifier::builder(Arc::clone(&roots)).build()?;
    let server = ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(vec![certificate.clone()], key.clone_key())?;
    let client = ClientConfig::builder()
        .with_root_certificates(Arc::clone(&roots))
        .with_client_auth_cert(vec![certificate], key)?;

    Ok((
        TlsAcceptor::from(Arc::new(server)),
        TlsConnector::from(Arc::new(client)),
    ))
}

async fn accept_loop(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    reports: mpsc::Sender<Vec<u8>>,
    delivery: Arc<Mutex<BidirectionalDelivery>>,
    config: Arc<PeerConfig>,
) -> Result<()> {
    loop {
        let (tcp, remote_addr) = listener.accept().await?;
        tcp.set_nodelay(true)?;
        let acceptor = acceptor.clone();
        let reports = reports.clone();
        let delivery = Arc::clone(&delivery);
        let config = Arc::clone(&config);
        let peer_id_for_log = config.peer_id.clone();
        tokio::spawn(async move {
            let result = async {
                let tls = time::timeout(Duration::from_secs(8), acceptor.accept(tcp))
                    .await
                    .context("TLS handshake timed out")??;
                verify_peer_pin(
                    tls.get_ref().1.peer_certificates(),
                    &config.expected_peer_sha256,
                )?;
                eprintln!(
                    "ARES_AUTHENTICATED local={} peer={} remote={remote_addr}",
                    local_name(config.local_is_a),
                    config.peer_id
                );
                receive_loop(tls, reports, delivery, config).await
            }
            .await;
            if let Err(error) = result {
                eprintln!(
                    "ARES_INBOUND_CLOSED peer={} error={error:#}",
                    peer_id_for_log
                );
            }
        });
    }
}

async fn connect_loop(
    connector: TlsConnector,
    mut reports: mpsc::Receiver<Vec<u8>>,
    delivery: Arc<Mutex<BidirectionalDelivery>>,
    config: Arc<PeerConfig>,
) -> Result<()> {
    let server_name = ServerName::try_from(config.peer_id.clone())
        .map_err(|_| anyhow!("peer id must be a valid DNS name for certificate SAN"))?;
    let mut sequence = 0_u64;
    loop {
        let tcp = match TcpStream::connect(config.peer_addr).await {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("ARES_CONNECT_RETRY peer={} error={error}", config.peer_id);
                time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        tcp.set_nodelay(true)?;
        let tls = match connector.connect(server_name.clone(), tcp).await {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("ARES_TLS_RETRY peer={} error={error}", config.peer_id);
                time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        verify_peer_pin(
            tls.get_ref().1.peer_certificates(),
            &config.expected_peer_sha256,
        )?;
        eprintln!(
            "ARES_OUTBOUND_AUTHENTICATED local={} peer={}",
            local_name(config.local_is_a),
            config.peer_id
        );

        let result = send_loop(tls, &mut reports, &delivery, &config, &mut sequence).await;
        if let Err(error) = result {
            eprintln!(
                "ARES_OUTBOUND_RETRY peer={} error={error:#}",
                config.peer_id
            );
        }
        time::sleep(Duration::from_millis(250)).await;
    }
}

async fn send_loop(
    stream: tokio_rustls::client::TlsStream<TcpStream>,
    reports: &mut mpsc::Receiver<Vec<u8>>,
    delivery: &Arc<Mutex<BidirectionalDelivery>>,
    config: &PeerConfig,
    sequence: &mut u64,
) -> Result<()> {
    let (_reader, mut writer) = tokio::io::split(stream);
    let mut ticker = time::interval(config.probe_interval);
    ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                *sequence = sequence.checked_add(1).ok_or_else(|| anyhow!("probe sequence exhausted"))?;
                let frame = encode_frame(
                    MessageKind::Probe,
                    config.session_id,
                    *sequence,
                    &[],
                )?;
                writer.write_all(&frame).await?;
                writer.flush().await?;
                let direction = local_to_peer(config.local_is_a);
                delivery.lock().map_err(|_| anyhow!("delivery mutex poisoned"))?
                    .direction_mut(direction).record_sent(*sequence)?;
            }
            report = reports.recv() => {
                let Some(frame) = report else { bail!("receiver report queue closed") };
                writer.write_all(&frame).await?;
                writer.flush().await?;
            }
        }
    }
}

async fn receive_loop(
    stream: tokio_rustls::server::TlsStream<TcpStream>,
    reports: mpsc::Sender<Vec<u8>>,
    delivery: Arc<Mutex<BidirectionalDelivery>>,
    config: Arc<PeerConfig>,
) -> Result<()> {
    let (mut reader, _writer) = tokio::io::split(stream);
    let (frame_tx, mut frame_rx) = mpsc::channel::<Vec<u8>>(64);
    tokio::spawn(async move {
        while let Ok(frame) = read_frame(&mut reader).await {
            if frame_tx.send(frame).await.is_err() {
                break;
            }
        }
    });

    let inbound_direction = peer_to_local(config.local_is_a);
    let mut expected = delivery
        .lock()
        .map_err(|_| anyhow!("delivery mutex poisoned"))?
        .direction(inbound_direction)
        .last_observed_sequence()
        .saturating_add(1);
    let deadline = time::sleep(config.observation_timeout);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            frame = frame_rx.recv() => {
                let Some(bytes) = frame else { bail!("peer stream closed") };
                let frame = decode_frame(&bytes)?;
                if frame.session_id != config.session_id {
                    bail!("frame belongs to a different session");
                }
                match frame.kind {
                    MessageKind::Probe => {
                        if frame.sequence >= expected {
                            while expected < frame.sequence {
                                if frame.sequence.saturating_sub(expected) > MAX_REPORT_GAP {
                                    bail!("peer probe gap exceeds safety bound");
                                }
                                apply_observed_missing(&delivery, inbound_direction, expected)?;
                                queue_report(&reports, &config, expected, REPORT_MISSING).await?;
                                expected = expected.saturating_add(1);
                            }
                            apply_observed_received(&delivery, inbound_direction, frame.sequence)?;
                            queue_report(&reports, &config, frame.sequence, REPORT_RECEIVED).await?;
                            expected = frame.sequence.saturating_add(1);
                            deadline.as_mut().reset(time::Instant::now() + config.observation_timeout);
                        }
                    }
                    MessageKind::Acknowledgement => {
                        if frame.payload.len() != 1 {
                            bail!("invalid receipt report payload");
                        }
                        let direction = local_to_peer(config.local_is_a);
                        let mut state = delivery.lock()
                            .map_err(|_| anyhow!("delivery mutex poisoned"))?;
                        match frame.payload[0] {
                            REPORT_RECEIVED => state.direction_mut(direction)
                                .record_remote_received(frame.sequence)?,
                            REPORT_MISSING => state.direction_mut(direction)
                                .record_remote_missing(frame.sequence)?,
                            _ => bail!("unknown receipt report value"),
                        }
                    }
                    MessageKind::Data => bail!("data frames are not enabled in the heartbeat runtime"),
                }
            }
            _ = &mut deadline => {
                apply_observed_missing(&delivery, inbound_direction, expected)?;
                queue_report(&reports, &config, expected, REPORT_MISSING).await?;
                expected = expected.saturating_add(1);
                deadline.as_mut().reset(time::Instant::now() + config.observation_timeout);
            }
        }
    }
}

async fn queue_report(
    reports: &mpsc::Sender<Vec<u8>>,
    config: &PeerConfig,
    sequence: u64,
    status: u8,
) -> Result<()> {
    let frame = encode_frame(
        MessageKind::Acknowledgement,
        config.session_id,
        sequence,
        &[status],
    )?;
    reports
        .send(frame)
        .await
        .context("outbound report queue closed")?;
    Ok(())
}

fn apply_observed_received(
    delivery: &Arc<Mutex<BidirectionalDelivery>>,
    direction: Direction,
    sequence: u64,
) -> Result<()> {
    let mut state = delivery
        .lock()
        .map_err(|_| anyhow!("delivery mutex poisoned"))?;
    match state
        .direction_mut(direction)
        .record_observed_received(sequence)
    {
        Ok(()) => Ok(()),
        Err(crate::ReceiptError::ReportSequenceNotIncreasing { .. }) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn apply_observed_missing(
    delivery: &Arc<Mutex<BidirectionalDelivery>>,
    direction: Direction,
    sequence: u64,
) -> Result<()> {
    let mut state = delivery
        .lock()
        .map_err(|_| anyhow!("delivery mutex poisoned"))?;
    match state
        .direction_mut(direction)
        .record_observed_missing(sequence)
    {
        Ok(()) => Ok(()),
        Err(crate::ReceiptError::ReportSequenceNotIncreasing { .. }) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<u8>> {
    let mut header = [0_u8; HEADER_LEN];
    reader.read_exact(&mut header).await?;
    let payload_len = u32::from_be_bytes([header[32], header[33], header[34], header[35]]) as usize;
    if payload_len > MAX_PAYLOAD_LEN {
        bail!("incoming frame exceeds payload limit");
    }
    let mut frame = Vec::with_capacity(HEADER_LEN + payload_len);
    frame.extend_from_slice(&header);
    frame.resize(HEADER_LEN + payload_len, 0);
    reader.read_exact(&mut frame[HEADER_LEN..]).await?;
    decode_frame(&frame)?;
    Ok(frame)
}

fn verify_peer_pin(certificates: Option<&[CertificateDer<'_>]>, expected: &[u8; 32]) -> Result<()> {
    let leaf = certificates
        .and_then(|certificates| certificates.first())
        .ok_or_else(|| anyhow!("peer did not present a certificate"))?;
    let actual: [u8; 32] = Sha256::digest(leaf.as_ref()).into();
    if actual != *expected {
        bail!("peer certificate SHA-256 pin mismatch");
    }
    Ok(())
}

fn local_to_peer(local_is_a: bool) -> Direction {
    if local_is_a {
        Direction::AToB
    } else {
        Direction::BToA
    }
}

fn peer_to_local(local_is_a: bool) -> Direction {
    if local_is_a {
        Direction::BToA
    } else {
        Direction::AToB
    }
}

fn local_name(local_is_a: bool) -> &'static str {
    if local_is_a {
        "a"
    } else {
        "b"
    }
}

/// Snapshot for a supervisor, CLI, or metrics exporter.
pub fn session_snapshot(
    delivery: &BidirectionalDelivery,
) -> (SessionState, DirectionState, DirectionState) {
    (
        delivery.state(),
        delivery.direction(Direction::AToB).state(),
        delivery.direction(Direction::BToA).state(),
    )
}
