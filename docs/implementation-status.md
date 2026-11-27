# ARES implementation status

Last updated: 2026-11-27.

## Implemented

| Area | Status | Boundary |
|---|---|---|
| Direction/session state | Implemented | Separate directional state; conservative aggregate |
| Frame codec and replay window | Implemented | Bounded framing and replay bookkeeping |
| Receipt ledger | Implemented | Sender-side authenticated reports plus direct receiver observations |
| Native two-peer runtime | Implemented as a proof of concept | Tokio TCP/TLS; independent outbound connections; CA validation and SHA-256 peer pins |
| Receiver telemetry | Implemented | Scheduled heartbeats, receiver-observed deadlines, reports over the opposite independent connection |
| Packet-drop harness | Implemented | Linux namespaces, veth, and tc netem; CI job executes real egress packet loss |
| CI | Configured | Format, Clippy, tests, binary build, docs, packet-drop experiment |

## Explicit limitations

- TCP/TLS only; Quinn/QUIC and deadline-bound datagrams are not implemented.
- Heartbeat deadline misses are liveness evidence, not proof that a specific IP packet was dropped.
- The runtime currently carries heartbeats and receipts, not application data, audio, or video.
- No discovery, NAT traversal, relay, path migration, browser runtime, persistent identity store, revocation, or key rotation.
- No offline-LAN claim until tested on real target devices and AP configurations.
- No WebRTC comparative reliability claim or production security audit.

## Evidence gates

1. Compile, lint, unit tests, docs.
2. Two-process mutual TLS and peer fingerprint enforcement.
3. Linux veth/tc netem directional packet-drop experiment.
4. Real device/LAN tests with internet disabled and realistic Wi-Fi isolation.
5. Reproducible WebRTC baseline comparison with absolute rates and confidence intervals.

The first three gates are represented by CI jobs and scripts, but are not considered passed until the corresponding run results are inspected. No simulated network result is reported as validation.
