# ARES implementation status

Last updated: 2026-11-27.

## Implemented

| Area | Status | Boundary |
|---|---|---|
| Direction/session state | Implemented | Separate directional state; conservative aggregate |
| Frame codec and replay window | Implemented | Bounded framing and replay bookkeeping |
| Receipt ledger | Implemented | Sender-side authenticated reports plus direct receiver observations |
| Native two-peer runtime | Implemented as a proof of concept | Mutual TLS, CA validation, exact SHA-256 peer pins, independent UDP heartbeat/report path |
| Datagram security | Implemented | HKDF-SHA-256 key separation and ChaCha20-Poly1305 authentication/encryption using an out-of-band session PSK |
| Receiver telemetry | Implemented | Scheduled heartbeats, receiver-observed deadlines, reports over reverse UDP |
| Packet-drop harness | Passed | Linux namespaces, veth, and tc netem; A egress 100% loss produced A→B Failed / B→A Healthy |
| CI | Passed on `136e0a4` | Format, Clippy, 51 tests, binary build, docs, and directional packet-drop job all green (run `46010224443`) |

## Explicit limitations

- The runtime is a heartbeat/reliability proof of concept, not a general data, audio, or video transport.
- UDP reports are not retransmitted; there is no ack-of-ack or report reorder buffer yet.
- A missed heartbeat is liveness evidence, not proof that a specific IP packet was dropped.
- No discovery, NAT traversal, relay, path migration, browser runtime, persistent identity store, revocation, or key rotation.
- No offline-LAN claim until tested on real target devices and AP configurations.
- No WebRTC comparative reliability claim or production security audit.

## Evidence gates

1. Compile, lint, unit tests, docs.
2. Two-process mutual TLS, exact peer pins, and AEAD-authenticated UDP.
3. Linux veth/tc netem directional packet-drop experiment.
4. Real device/LAN tests with internet disabled and realistic Wi-Fi isolation.
5. Reproducible WebRTC baseline comparison with absolute rates and confidence intervals.

Evidence snapshot (2026-11-27): gates 1–3 passed on commit `136e0a4` in GitHub Actions run `46010224443`. The real veth test applied 100% egress loss on A only and observed `session=OneWay a_to_b=Failed b_to_a=Healthy` at A. This validates that one controlled Linux topology and impairment profile only; it does not establish real Wi-Fi/LAN behavior, mirrored B→A behavior, simultaneous outage behavior, or a WebRTC improvement. No simulated network result is reported as validation.
