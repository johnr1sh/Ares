//! Authenticated two-peer runtime: mutual TLS bootstrap plus AEAD-protected UDP.
//!
//! TCP is used only to authenticate both peers and pin their certificates.
//! Heartbeats and receiver reports use UDP so a one-way IP impairment does not
//! also suppress the surviving direction through TCP acknowledgement traffic.
//! A unique session PSK is provisioned out of band and expanded into distinct
//! per-direction, per-message-kind ChaCha20-Poly1305 keys.

use crate::{
    decode_frame, encode_frame, BidirectionalDelivery, Direction, DirectionHealth, DirectionState,
    MessageKind, ReceiptPolicy, SessionHealth, SessionState,
};
use anyhow::{anyhow, bail, Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    future,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream, UdpSocket},
    sync::Notify,
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

const AUTH_INBOUND: u8 = 0b01;
const AUTH_OUTBOUND: u8 = 0b10;
const MAX_REPORT_GAP: u64 = 4096;
const MAX_PENDING_INBOUND: usize = 256;
const MAX_UDP_PACKET: usize = 1200;
const REPORT_RECEIVED: u8 = 0;
const REPORT_MISSING: u8 = 1;
const DATAGRAM_HEADER_LEN: usize = 9;
const AEAD_TAG_LEN: usize = 16;

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
    /// Unique 32-byte secret provisioned to both peers for this session.
    pub shared_session_key: [u8; 32],
    pub probe_interval: Duration,
    pub observation_timeout: Duration,
    pub receipt_policy: ReceiptPolicy,
}

#[derive(Default)]
struct AuthGate {
    flags: AtomicU8,
    notify: Notify,
}

impl AuthGate {
    fn mark(&self, flag: u8) {
        self.flags.fetch_or(flag, Ordering::AcqRel);
        self.notify.notify_waiters();
    }

    async fn wait_for_mutual_auth(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.flags.load(Ordering::Acquire) == (AUTH_INBOUND | AUTH_OUTBOUND) {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Clone)]
struct DirectionalKeys {
    a_to_b_probe: [u8; 32],
    a_to_b_receipt: [u8; 32],
    b_to_a_probe: [u8; 32],
    b_to_a_receipt: [u8; 32],
}

impl DirectionalKeys {
    fn new(shared: &[u8; 32], session_id: &[u8; 16]) -> Result<Self> {
        Ok(Self {
            a_to_b_probe: derive_key(shared, session_id, b"ARES/v1/A-to-B/probe")?,
            a_to_b_receipt: derive_key(shared, session_id, b"ARES/v1/A-to-B/receipt")?,
            b_to_a_probe: derive_key(shared, session_id, b"ARES/v1/B-to-A/probe")?,
            b_to_a_receipt: derive_key(shared, session_id, b"ARES/v1/B-to-A/receipt")?,
        })
    }

    fn get(&self, direction: Direction, kind: MessageKind) -> &[u8; 32] {
        match (direction, kind) {
            (Direction::AToB, MessageKind::Probe) => &self.a_to_b_probe,
            (Direction::AToB, MessageKind::Acknowledgement) => &self.a_to_b_receipt,
            (Direction::BToA, MessageKind::Probe) => &self.b_to_a_probe,
            (Direction::BToA, MessageKind::Acknowledgement) => &self.b_to_a_receipt,
            (_, MessageKind::Data) => unreachable!("data is not enabled in the heartbeat runtime"),
        }
    }
}

/// Local expiry policy for evidence that was previously fresh. This layer
/// never invents a directional failure: silence downgrades to Degraded, and a
/// stale direction needs multiple fresh positive observations before returning
/// to Healthy.
#[derive(Debug, Clone, Copy, Default)]
struct DirectionEvidenceFreshness {
    last_positive_count: u64,
    last_missing_count: u64,
    last_positive_at: Option<time::Instant>,
    last_missing_at: Option<time::Instant>,
    last_any_at: Option<time::Instant>,
    stale: bool,
    recovery_required: bool,
    recovery_successes: u32,
}

impl DirectionEvidenceFreshness {
    fn evaluate(
        &mut self,
        underlying: DirectionState,
        positive_count: u64,
        missing_count: u64,
        now: time::Instant,
        stale_after: Duration,
        required_successes: u32,
    ) -> (DirectionState, bool) {
        let new_positive = positive_count.saturating_sub(self.last_positive_count);
        let new_missing = missing_count.saturating_sub(self.last_missing_count);
        self.last_positive_count = self.last_positive_count.max(positive_count);
        self.last_missing_count = self.last_missing_count.max(missing_count);

        if new_positive > 0 {
            self.stale = false;
            self.last_positive_at = Some(now);
            self.last_any_at = Some(now);
            if self.recovery_required {
                self.recovery_successes = self
                    .recovery_successes
                    .saturating_add(new_positive.min(u64::from(u32::MAX)) as u32);
                if self.recovery_successes >= required_successes {
                    self.recovery_required = false;
                    self.recovery_successes = 0;
                }
            }
        }

        if new_missing > 0 {
            self.last_missing_at = Some(now);
            self.last_any_at = Some(now);
            // Fresh receiver-observed absence refreshes negative evidence only.
            self.stale = false;
            self.recovery_successes = 0;
        }

        let relevant_time = match underlying {
            DirectionState::Healthy | DirectionState::Recovering => self.last_positive_at,
            DirectionState::Failed => self.last_missing_at,
            DirectionState::Degraded => self.last_any_at,
            DirectionState::Unknown | DirectionState::Starting | DirectionState::Closed => None,
        };

        if !self.stale
            && relevant_time
                .map(|observed| now.duration_since(observed) >= stale_after)
                .unwrap_or(false)
        {
            self.stale = true;
            self.recovery_required = true;
            self.recovery_successes = 0;
        }

        let effective = if (self.stale
            && matches!(
                underlying,
                DirectionState::Healthy | DirectionState::Failed | DirectionState::Recovering
            )) || (self.recovery_required && underlying == DirectionState::Healthy) {
            DirectionState::Degraded
        } else {
            underlying
        };
        let fresh = !self.stale
            && !(self.recovery_required && underlying == DirectionState::Healthy)
            && relevant_time
                .map(|observed| now.duration_since(observed) < stale_after)
                .unwrap_or(false);

        (effective, fresh)
    }
}

fn derive_key(shared: &[u8; 32], session_id: &[u8; 16], label: &[u8]) -> Result<[u8; 32]> {
    let hkdf = Hkdf::<Sha256>::new(Some(session_id), shared);
    let mut key = [0_u8; 32];
    hkdf.expand(label, &mut key)
        .map_err(|_| anyhow!("HKDF output length rejected"))?;
    Ok(key)
}

pub async fn run_peer(config: PeerConfig) -> Result<()> {
    if config.probe_interval.is_zero() || config.observation_timeout <= config.probe_interval {
        bail!("observation timeout must exceed a non-zero probe interval");
    }

    let (acceptor, connector) = build_tls(&config)?;
    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("bind TCP {}", config.bind_addr))?;
    let udp = Arc::new(
        UdpSocket::bind(config.bind_addr)
            .await
            .with_context(|| format!("bind UDP {}", config.bind_addr))?,
    );
    let keys = Arc::new(DirectionalKeys::new(
        &config.shared_session_key,
        &config.session_id,
    )?);
    let delivery = Arc::new(Mutex::new(BidirectionalDelivery::new(
        config.receipt_policy,
    )?));
    let config = Arc::new(config);
    let auth = Arc::new(AuthGate::default());

    let monitor_delivery = Arc::clone(&delivery);
    let monitor_role = config.local_is_a;
    let freshness_timeout = config.observation_timeout.saturating_mul(3);
    let recovery_required = config.receipt_policy.recovery_successes;
    tokio::spawn(async move {
        let mut ticker = time::interval(Duration::from_secs(1));
        let mut freshness = [DirectionEvidenceFreshness::default(); 2];
        loop {
            ticker.tick().await;
            if let Ok(state) = monitor_delivery.lock() {
                let now = time::Instant::now();
                let directions = [Direction::AToB, Direction::BToA];
                let mut directional = [DirectionState::Unknown; 2];
                let mut evidence_fresh = [false; 2];

                for (index, direction) in directions.into_iter().enumerate() {
                    let ledger = state.direction(direction);
                    (directional[index], evidence_fresh[index]) = freshness[index].evaluate(
                        ledger.state(),
                        ledger.confirmed_received(),
                        ledger.confirmed_missing(),
                        now,
                        freshness_timeout,
                        recovery_required,
                    );
                }

                let session = SessionHealth {
                    a_to_b: DirectionHealth {
                        state: directional[0],
                        ..DirectionHealth::default()
                    },
                    b_to_a: DirectionHealth {
                        state: directional[1],
                        ..DirectionHealth::default()
                    },
                }
                .state();
                println!(
                    "ARES_STATE local={} session={:?} a_to_b={:?} b_to_a={:?} a_to_b_fresh={} b_to_a_fresh={}",
                    local_name(monitor_role),
                    session,
                    directional[0],
                    directional[1],
                    evidence_fresh[0],
                    evidence_fresh[1]
                );
            }
        }
    });

    let accept_task = accept_loop(listener, acceptor, Arc::clone(&auth), Arc::clone(&config));
    let connect_task = connect_loop(connector, Arc::clone(&auth), Arc::clone(&config));
    let udp_task = udp_supervisor(udp, delivery, keys, Arc::clone(&auth), config);

    tokio::select! {
        result = accept_task => result,
        result = connect_task => result,
        result = udp_task => result,
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
    auth: Arc<AuthGate>,
    config: Arc<PeerConfig>,
) -> Result<()> {
    loop {
        let (tcp, remote_addr) = listener.accept().await?;
        tcp.set_nodelay(true)?;
        let acceptor = acceptor.clone();
        let auth = Arc::clone(&auth);
        let config = Arc::clone(&config);
        tokio::spawn(async move {
            let result = async {
                let tls = time::timeout(Duration::from_secs(8), acceptor.accept(tcp))
                    .await
                    .context("inbound TLS handshake timed out")??;
                verify_peer_pin(
                    tls.get_ref().1.peer_certificates(),
                    &config.expected_peer_sha256,
                )?;
                eprintln!(
                    "ARES_AUTHENTICATED local={} peer={} remote={remote_addr}",
                    local_name(config.local_is_a),
                    config.peer_id
                );
                auth.mark(AUTH_INBOUND);
                let _tls = tls;
                future::pending::<()>().await;
                #[allow(unreachable_code)]
                Ok::<(), anyhow::Error>(())
            }
            .await;
            if let Err(error) = result {
                eprintln!("ARES_INBOUND_AUTH_FAILED error={error:#}");
            }
        });
    }
}

async fn connect_loop(
    connector: TlsConnector,
    auth: Arc<AuthGate>,
    config: Arc<PeerConfig>,
) -> Result<()> {
    let server_name = ServerName::try_from(config.peer_id.clone())
        .map_err(|_| anyhow!("peer id must be a valid DNS name for certificate SAN"))?;
    loop {
        let tcp = match time::timeout(Duration::from_secs(5), TcpStream::connect(config.peer_addr))
            .await
        {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => {
                eprintln!("ARES_CONNECT_RETRY peer={} error={error}", config.peer_id);
                time::sleep(Duration::from_millis(500)).await;
                continue;
            }
            Err(_) => {
                eprintln!("ARES_CONNECT_RETRY peer={} timeout", config.peer_id);
                time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        tcp.set_nodelay(true)?;
        let tls = match time::timeout(
            Duration::from_secs(8),
            connector.connect(server_name.clone(), tcp),
        )
        .await
        {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => {
                eprintln!("ARES_TLS_RETRY peer={} error={error}", config.peer_id);
                time::sleep(Duration::from_millis(500)).await;
                continue;
            }
            Err(_) => {
                eprintln!("ARES_TLS_RETRY peer={} timeout", config.peer_id);
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
        auth.mark(AUTH_OUTBOUND);
        let _tls = tls;
        future::pending::<()>().await;
    }
}

async fn udp_supervisor(
    socket: Arc<UdpSocket>,
    delivery: Arc<Mutex<BidirectionalDelivery>>,
    keys: Arc<DirectionalKeys>,
    auth: Arc<AuthGate>,
    config: Arc<PeerConfig>,
) -> Result<()> {
    auth.wait_for_mutual_auth().await;
    eprintln!(
        "ARES_UDP_SESSION_READY local={} peer={}",
        local_name(config.local_is_a),
        config.peer_id
    );
    let sender = send_loop(
        Arc::clone(&socket),
        Arc::clone(&delivery),
        Arc::clone(&keys),
        Arc::clone(&config),
    );
    let receiver = receive_loop(socket, delivery, keys, config);
    tokio::try_join!(sender, receiver)?;
    Ok(())
}

async fn send_loop(
    socket: Arc<UdpSocket>,
    delivery: Arc<Mutex<BidirectionalDelivery>>,
    keys: Arc<DirectionalKeys>,
    config: Arc<PeerConfig>,
) -> Result<()> {
    let direction = local_to_peer(config.local_is_a);
    let key = keys.get(direction, MessageKind::Probe);
    let mut sequence = 0_u64;
    let mut ticker = time::interval(config.probe_interval);
    ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        sequence = sequence
            .checked_add(1)
            .ok_or_else(|| anyhow!("probe sequence exhausted"))?;
        let frame = encode_frame(MessageKind::Probe, config.session_id, sequence, &[])?;
        let datagram = seal_frame(&frame, key, &config.session_id)?;
        let sent = socket.send_to(&datagram, config.peer_addr).await?;
        if sent != datagram.len() {
            bail!("UDP datagram was not sent atomically");
        }
        delivery
            .lock()
            .map_err(|_| anyhow!("delivery mutex poisoned"))?
            .direction_mut(direction)
            .record_sent(sequence)?;
    }
}

async fn receive_loop(
    socket: Arc<UdpSocket>,
    delivery: Arc<Mutex<BidirectionalDelivery>>,
    keys: Arc<DirectionalKeys>,
    config: Arc<PeerConfig>,
) -> Result<()> {
    let inbound_direction = peer_to_local(config.local_is_a);
    let outbound_direction = local_to_peer(config.local_is_a);
    let mut expected = delivery
        .lock()
        .map_err(|_| anyhow!("delivery mutex poisoned"))?
        .direction(inbound_direction)
        .last_observed_sequence()
        .saturating_add(1);
    let mut pending = BTreeSet::new();
    let deadline = time::sleep(config.observation_timeout);
    tokio::pin!(deadline);
    let mut buffer = [0_u8; MAX_UDP_PACKET];

    loop {
        tokio::select! {
            received = socket.recv_from(&mut buffer) => {
                let (length, remote_addr) = received?;
                if remote_addr != config.peer_addr {
                    continue;
                }
                let kind = match datagram_kind(&buffer[..length]) {
                    Ok(kind) if kind != MessageKind::Data => kind,
                    _ => continue,
                };
                let plaintext = match open_datagram(
                    &buffer[..length],
                    keys.get(inbound_direction, kind),
                    &config.session_id,
                ) {
                    Ok(frame) => frame,
                    Err(_) => continue,
                };
                let frame = decode_frame(&plaintext)?;
                if frame.session_id != config.session_id {
                    continue;
                }
                match frame.kind {
                    MessageKind::Probe => {
                        if frame.sequence < expected
                            || frame.sequence.saturating_sub(expected) > MAX_REPORT_GAP
                            || pending.len() >= MAX_PENDING_INBOUND
                        {
                            continue;
                        }
                        pending.insert(frame.sequence);
                        while pending.remove(&expected) {
                            apply_observed_received(&delivery, inbound_direction, expected)?;
                            send_report(
                                &socket,
                                &keys,
                                &config,
                                outbound_direction,
                                expected,
                                REPORT_RECEIVED,
                            ).await?;
                            expected = expected.saturating_add(1);
                            deadline.as_mut().reset(time::Instant::now() + config.observation_timeout);
                        }
                    }
                    MessageKind::Acknowledgement => {
                        if frame.payload.len() != 1 {
                            continue;
                        }
                        let mut state = delivery
                            .lock()
                            .map_err(|_| anyhow!("delivery mutex poisoned"))?;
                        let result = match frame.payload[0] {
                            REPORT_RECEIVED => state
                                .direction_mut(outbound_direction)
                                .record_remote_received(frame.sequence),
                            REPORT_MISSING => state
                                .direction_mut(outbound_direction)
                                .record_remote_missing(frame.sequence),
                            _ => continue,
                        };
                        match result {
                            Ok(()) => {}
                            Err(crate::ReceiptError::UnknownOrAlreadyReported(_))
                            | Err(crate::ReceiptError::ReportSequenceNotIncreasing { .. }) => {}
                            Err(error) => return Err(error.into()),
                        }
                    }
                    MessageKind::Data => continue,
                }
            }
            _ = &mut deadline => {
                apply_observed_missing(&delivery, inbound_direction, expected)?;
                send_report(
                    &socket,
                    &keys,
                    &config,
                    outbound_direction,
                    expected,
                    REPORT_MISSING,
                ).await?;
                expected = expected.saturating_add(1);
                while pending.remove(&expected) {
                    apply_observed_received(&delivery, inbound_direction, expected)?;
                    send_report(
                        &socket,
                        &keys,
                        &config,
                        outbound_direction,
                        expected,
                        REPORT_RECEIVED,
                    ).await?;
                    expected = expected.saturating_add(1);
                }
                deadline.as_mut().reset(time::Instant::now() + config.observation_timeout);
            }
        }
    }
}

async fn send_report(
    socket: &UdpSocket,
    keys: &DirectionalKeys,
    config: &PeerConfig,
    direction: Direction,
    sequence: u64,
    status: u8,
) -> Result<()> {
    let frame = encode_frame(
        MessageKind::Acknowledgement,
        config.session_id,
        sequence,
        &[status],
    )?;
    let datagram = seal_frame(
        &frame,
        keys.get(direction, MessageKind::Acknowledgement),
        &config.session_id,
    )?;
    let sent = socket.send_to(&datagram, config.peer_addr).await?;
    if sent != datagram.len() {
        bail!("UDP receipt datagram was not sent atomically");
    }
    Ok(())
}

fn seal_frame(frame: &[u8], key: &[u8; 32], session_id: &[u8; 16]) -> Result<Vec<u8>> {
    let decoded = decode_frame(frame)?;
    let mut header = [0_u8; DATAGRAM_HEADER_LEN];
    header[0] = decoded.kind as u8;
    header[1..].copy_from_slice(&decoded.sequence.to_be_bytes());
    let nonce = nonce_for(decoded.sequence);
    let mut aad = [0_u8; 16 + DATAGRAM_HEADER_LEN];
    aad[..16].copy_from_slice(session_id);
    aad[16..].copy_from_slice(&header);
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| anyhow!("invalid ChaCha20-Poly1305 key"))?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: frame,
                aad: &aad,
            },
        )
        .map_err(|_| anyhow!("UDP frame encryption failed"))?;
    let mut datagram = Vec::with_capacity(DATAGRAM_HEADER_LEN + ciphertext.len());
    datagram.extend_from_slice(&header);
    datagram.extend_from_slice(&ciphertext);
    Ok(datagram)
}

fn open_datagram(datagram: &[u8], key: &[u8; 32], session_id: &[u8; 16]) -> Result<Vec<u8>> {
    if datagram.len() < DATAGRAM_HEADER_LEN + AEAD_TAG_LEN {
        bail!("truncated encrypted UDP datagram");
    }
    let kind = MessageKind::try_from(datagram[0])?;
    let sequence = u64::from_be_bytes(
        datagram[1..DATAGRAM_HEADER_LEN]
            .try_into()
            .map_err(|_| anyhow!("invalid UDP sequence header"))?,
    );
    let nonce = nonce_for(sequence);
    let mut aad = [0_u8; 16 + DATAGRAM_HEADER_LEN];
    aad[..16].copy_from_slice(session_id);
    aad[16..].copy_from_slice(&datagram[..DATAGRAM_HEADER_LEN]);
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| anyhow!("invalid ChaCha20-Poly1305 key"))?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &datagram[DATAGRAM_HEADER_LEN..],
                aad: &aad,
            },
        )
        .map_err(|_| anyhow!("UDP datagram authentication failed"))?;
    let frame = decode_frame(&plaintext)?;
    if frame.kind != kind || frame.sequence != sequence {
        bail!("encrypted datagram header does not match authenticated frame");
    }
    Ok(plaintext)
}

fn datagram_kind(datagram: &[u8]) -> Result<MessageKind> {
    let kind = *datagram
        .first()
        .ok_or_else(|| anyhow!("empty UDP datagram"))?;
    Ok(MessageKind::try_from(kind)?)
}

fn nonce_for(sequence: u64) -> [u8; 12] {
    let mut nonce = [0_u8; 12];
    nonce[4..].copy_from_slice(&sequence.to_be_bytes());
    nonce
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_probe_round_trips_with_session_binding() {
        let key = [0x5a; 32];
        let session_id = [0x11; 16];
        let frame = encode_frame(MessageKind::Probe, session_id, 42, &[]).unwrap();
        let datagram = seal_frame(&frame, &key, &session_id).unwrap();
        let kind = datagram_kind(&datagram).unwrap();
        assert_eq!(kind, MessageKind::Probe);
        let plaintext = open_datagram(&datagram, &key, &session_id).unwrap();
        let decoded = decode_frame(&plaintext).unwrap();
        assert_eq!(decoded.kind, MessageKind::Probe);
        assert_eq!(decoded.sequence, 42);
        assert_eq!(decoded.session_id, session_id);
    }

    #[test]
    fn encrypted_datagram_rejects_ciphertext_tampering_and_wrong_session() {
        let key = [0x33; 32];
        let session_id = [0x22; 16];
        let frame = encode_frame(
            MessageKind::Acknowledgement,
            session_id,
            7,
            &[REPORT_MISSING],
        )
        .unwrap();
        let mut datagram = seal_frame(&frame, &key, &session_id).unwrap();
        let last = datagram.len() - 1;
        datagram[last] ^= 0x80;
        assert!(open_datagram(&datagram, &key, &session_id).is_err());

        let clean = seal_frame(&frame, &key, &session_id).unwrap();
        assert!(open_datagram(&clean, &key, &[0x44; 16]).is_err());
    }

    #[test]
    fn session_keys_are_separated_by_direction_and_message_kind() {
        let keys = DirectionalKeys::new(&[0x77; 32], &[0x88; 16]).unwrap();
        assert_ne!(
            keys.get(Direction::AToB, MessageKind::Probe),
            keys.get(Direction::AToB, MessageKind::Acknowledgement)
        );
        assert_ne!(
            keys.get(Direction::AToB, MessageKind::Probe),
            keys.get(Direction::BToA, MessageKind::Probe)
        );
    }
    #[test]
    fn stale_healthy_evidence_requires_two_new_confirmations_to_recover() {
        let now = time::Instant::now();
        let stale_after = Duration::from_secs(3);
        let mut freshness = DirectionEvidenceFreshness::default();

        assert_eq!(
            freshness.evaluate(DirectionState::Healthy, 4, 0, now, stale_after, 2),
            (DirectionState::Healthy, true)
        );
        assert_eq!(
            freshness.evaluate(
                DirectionState::Healthy,
                4,
                0,
                now + stale_after + Duration::from_millis(1),
                stale_after,
                2,
            ),
            (DirectionState::Degraded, false)
        );
        assert_eq!(
            freshness.evaluate(
                DirectionState::Healthy,
                5,
                0,
                now + stale_after + Duration::from_millis(200),
                stale_after,
                2,
            ),
            (DirectionState::Degraded, false)
        );
        assert_eq!(
            freshness.evaluate(
                DirectionState::Healthy,
                6,
                0,
                now + stale_after + Duration::from_millis(400),
                stale_after,
                2,
            ),
            (DirectionState::Healthy, true)
        );
    }

    #[test]
    fn stale_failure_evidence_downgrades_and_fresh_missing_reports_restore_failure() {
        let now = time::Instant::now();
        let stale_after = Duration::from_secs(3);
        let mut freshness = DirectionEvidenceFreshness::default();

        assert_eq!(
            freshness.evaluate(DirectionState::Failed, 0, 3, now, stale_after, 2),
            (DirectionState::Failed, true)
        );
        assert_eq!(
            freshness.evaluate(
                DirectionState::Failed,
                0,
                3,
                now + stale_after + Duration::from_millis(1),
                stale_after,
                2,
            ),
            (DirectionState::Degraded, false)
        );
        assert_eq!(
            freshness.evaluate(
                DirectionState::Failed,
                0,
                4,
                now + stale_after + Duration::from_millis(200),
                stale_after,
                2,
            ),
            (DirectionState::Failed, true)
        );
    }

    #[test]
    fn stale_path_is_not_called_one_way_when_the_reverse_path_is_unknown() {
        let now = time::Instant::now();
        let stale_after = Duration::from_secs(3);
        let mut a_to_b = DirectionEvidenceFreshness::default();
        let mut b_to_a = DirectionEvidenceFreshness::default();

        assert_eq!(
            a_to_b
                .evaluate(DirectionState::Healthy, 10, 0, now, stale_after, 2)
                .0,
            DirectionState::Healthy
        );
        assert_eq!(
            b_to_a
                .evaluate(DirectionState::Healthy, 10, 0, now, stale_after, 2)
                .0,
            DirectionState::Healthy
        );

        let later = now + stale_after + Duration::from_millis(1);
        let a_to_b_state = a_to_b
            .evaluate(DirectionState::Healthy, 10, 0, later, stale_after, 2)
            .0;
        let b_to_a_state = b_to_a
            .evaluate(DirectionState::Failed, 0, 2, later, stale_after, 2)
            .0;
        let session = SessionHealth {
            a_to_b: DirectionHealth {
                state: a_to_b_state,
                ..DirectionHealth::default()
            },
            b_to_a: DirectionHealth {
                state: b_to_a_state,
                ..DirectionHealth::default()
            },
        }
        .state();
        assert_eq!(session, SessionState::Degraded);
    }
    #[tokio::test]
    async fn auth_gate_waits_for_both_directions_without_losing_wakeups() {
        let gate = Arc::new(AuthGate::default());
        let waiter_gate = Arc::clone(&gate);
        let waiter = tokio::spawn(async move {
            waiter_gate.wait_for_mutual_auth().await;
        });

        tokio::task::yield_now().await;
        gate.mark(AUTH_INBOUND);
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());

        gate.mark(AUTH_OUTBOUND);
        time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("mutual-auth gate waiter was not notified")
            .expect("mutual-auth gate task panicked");
    }

    #[test]
    fn fresh_negative_report_does_not_bypass_stale_recovery_hysteresis() {
        let now = time::Instant::now();
        let stale_after = Duration::from_secs(3);
        let mut freshness = DirectionEvidenceFreshness::default();

        freshness.evaluate(DirectionState::Healthy, 4, 0, now, stale_after, 2);
        assert_eq!(
            freshness.evaluate(
                DirectionState::Healthy,
                4,
                0,
                now + stale_after + Duration::from_millis(1),
                stale_after,
                2,
            ),
            (DirectionState::Degraded, false)
        );

        assert_eq!(
            freshness.evaluate(
                DirectionState::Degraded,
                4,
                1,
                now + stale_after + Duration::from_millis(100),
                stale_after,
                2,
            ),
            (DirectionState::Degraded, true)
        );
        assert_eq!(
            freshness.evaluate(
                DirectionState::Healthy,
                5,
                1,
                now + stale_after + Duration::from_millis(200),
                stale_after,
                2,
            ),
            (DirectionState::Degraded, false)
        );
        assert_eq!(
            freshness.evaluate(
                DirectionState::Healthy,
                6,
                1,
                now + stale_after + Duration::from_millis(300),
                stale_after,
                2,
            ),
            (DirectionState::Healthy, true)
        );
    }
}
