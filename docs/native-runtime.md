# Native two-peer runtime

## Scope

The native runtime is an authenticated heartbeat and directional-health proof
of concept. It is not yet a general-purpose media or data transport.

Each process:
- opens an outbound TCP/TLS connection to its peer and accepts the peer's
  independent outbound connection;
- requires a CA-validated client certificate and checks the peer leaf
  certificate against a SHA-256 pin on both accepted and outbound connections;
- waits for both TLS directions to authenticate before starting UDP telemetry;
- sends sequenced heartbeats and receiver reports as AEAD-protected UDP datagrams;
- derives separate ChaCha20-Poly1305 keys for each direction and message kind
  from a unique, out-of-band 32-byte session PSK using HKDF-SHA-256;
- observes inbound heartbeats at the receiver and emits received/missing
  reports over UDP in the reverse direction;
- maintains independent A→B and B→A evidence.

UDP is intentional: TCP's acknowledgement traffic travels in the reverse
direction, so a one-way packet blackhole can stall both directions of a
full-duplex TCP stream, even when two TCP connections are used.

## Build and launch

    cargo build --bin ares-peer

The runtime expects DER-encoded CA, leaf certificate, and PKCS#8 private key
files. The leaf certificate must contain the peer's DNS name (a or b) as a
SAN and permit both clientAuth and serverAuth. Both peers must trust the same
CA and pin the other peer's exact leaf SHA-256 fingerprint. Both peers must
also receive the same unique 32-byte session key and 16-byte session ID over a
secure provisioning channel.

    ares-peer a 192.0.2.10:41001 192.0.2.11:41001 b ca.der a.der a-key.der <b-cert-sha256> <session-id-hex> <shared-session-key-hex>
    ares-peer b 192.0.2.11:41001 192.0.2.10:41001 a ca.der b.der b-key.der <a-cert-sha256> <same-session-id> <same-shared-session-key>

Never reuse a session key with the same session ID for a restarted session:
the per-direction, per-message-kind AEAD nonce space is monotonic within a
session. The runtime does not yet implement a pairing protocol or automatic
key rotation.

## Directional evidence semantics

- A locally accepted UDP send is pending evidence, not delivery.
- An inbound heartbeat directly observed by this endpoint proves that inbound
  direction was working at the observation time.
- A missing heartbeat is emitted by the receiver only after the configured
  observation deadline. The report is sent as an authenticated UDP datagram
  over the independent reverse IP direction.
- A one-way classification requires one direction to be receiver-confirmed
  healthy and the other to have crossed the configured explicit missing-report
  failure threshold.
- A missing heartbeat means the scheduled application heartbeat was not
  observed before its deadline. It does not identify whether the cause was
  packet loss, congestion, a paused process, or a failed path.
- Out-of-order and duplicate observations are rejected by the monotonic
  sequence ledgers. The current prototype does not yet include a full UDP
  retransmission/ack-of-ack protocol or a report reorder buffer.

The runtime currently carries heartbeats and receipts, not application data,
audio, or video. It does not yet provide discovery, NAT traversal, relay
selection, path migration, browser support, offline-LAN certification,
certificate revocation, durable identity storage, production pairing, or a
security audit.

## Real packet-drop test

On Linux with root privileges, iproute2, OpenSSL, and a built binary:

    cargo build --bin ares-peer
    sudo bash scripts/test-directional-packet-drop.sh

The script creates two network namespaces connected by a veth pair, starts two
independent peer processes with mutually authenticated TLS and a fresh
session PSK, waits for a bidirectionally healthy baseline, then applies
tc netem loss 100% to A's egress interface only. It passes only if endpoint A
reports:

    session=OneWay a_to_b=Failed b_to_a=Healthy

The CI job runs this same network-layer experiment. A passing unit test or an
application-level drop simulation is not a substitute for this experiment.
