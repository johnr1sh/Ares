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
| Packet-drop harness | Passed | Both one-way directions, bidirectional blackhole ambiguity, and recovery tested with Linux namespaces/veth/tc netem |
| CI | Full rerun pending | Expanded network experiment passed on `d9bdf22` (run `46014398124`); format/lint/test rerun is in progress on `3334f89` (run `46014658811`) |

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

Evidence snapshot (2026-11-27): the expanded real veth test passed in run `46014398124` on `d9bdf22`. It validated A→B-only loss, B→A-only loss, both-directions-down ambiguity, and recovery after each impairment. During B→A-only loss, B correctly reported `OneWay` with B→A Failed/A→B Healthy; A conservatively downgraded stale A→B positive evidence and reported `Degraded`. During a bidirectional blackhole, both endpoints reported `Degraded`, not a falsely certain OneWay or Unavailable state. Both returned to BidirectionalHealthy after impairment removal. The run's format check failed on a formatting-only difference; that was corrected in `3334f89`, whose complete CI run is pending. Physical-device offline-LAN validation remains NOT RUN.
