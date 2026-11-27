use ares_core::{run_peer, PeerConfig};
use std::{env, net::SocketAddr, process, str::FromStr};

fn decode_hex<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    let bytes = hex::decode(value).map_err(|error| format!("{label}: {error}"))?;
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        format!("{label} must be exactly {} bytes, got {}", N, bytes.len())
    })
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("ARES_FATAL {error:#}");
        process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 10 {
        anyhow::bail!(
            "usage: ares-peer <a|b> <bind-addr> <peer-addr> <peer-id> <ca.der> <cert.der> <key.der> <peer-sha256-hex> <session-id-hex>"
        );
    }
    let local_is_a = match args[1].as_str() {
        "a" => true,
        "b" => false,
        _ => anyhow::bail!("role must be a or b"),
    };
    let expected_peer_id = if local_is_a { "b" } else { "a" };
    if args[4] != expected_peer_id {
        anyhow::bail!("peer id must be {expected_peer_id} for role {}", args[1]);
    }
    let bind_addr = SocketAddr::from_str(&args[2])?;
    let peer_addr = SocketAddr::from_str(&args[3])?;
    let expected_peer_sha256 =
        decode_hex::<32>(&args[8], "peer SHA-256 fingerprint").map_err(anyhow::Error::msg)?;
    let session_id = decode_hex::<16>(&args[9], "session id").map_err(anyhow::Error::msg)?;
    let config = PeerConfig {
        local_is_a,
        bind_addr,
        peer_addr,
        peer_id: args[4].clone(),
        ca_certificate_der: std::fs::read(&args[5])?,
        local_certificate_der: std::fs::read(&args[6])?,
        local_private_key_der: std::fs::read(&args[7])?,
        expected_peer_sha256,
        session_id,
        probe_interval: std::time::Duration::from_millis(200),
        observation_timeout: std::time::Duration::from_millis(900),
        receipt_policy: ares_core::ReceiptPolicy::default(),
    };
    run_peer(config).await
}
