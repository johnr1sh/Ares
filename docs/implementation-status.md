# ARES implementation status

Last updated: 2026-11-27.

## Implemented

| Area | Status | Boundary |
|---|---|---|
| Direction/session state | Implemented | Separate A→B and B→A state; conservative aggregate classification |
| Frame codec and replay window | Implemented | Bounded framing and fixed-memory replay bookkeeping |
| Receipt ledger | Implemented | Sender-side remote receipts plus direct receiver observations |
| Native two-peer runtime | Implemented as a proof of concept | Mutual TLS, CA validation, pinned peer certificates, independent AEAD-protected UDP telemetry |
| Datagram security | Implemented | HKDF-SHA-256 directional/message-kind key separation and ChaCha20-Poly1305 using an out-of-band session PSK |
| Evidence freshness | Implemented | Three-observation-timeout expiry; stale evidence is Degraded; positive recovery has a separate confirmation latch |
| Authentication gate | Implemented | Waits for inbound and outbound mutual TLS authentication without a Notify lost-wakeup race |
| Packet-drop harness | Passed | Mirrored one-way impairments, total bidirectional blackhole ambiguity, and recovery using Linux namespaces/veth/tc netem |
| CI | Passed on `c02e6d0` | Format, shell syntax, LAN provisioning smoke test, Clippy, 56 tests, binary/docs builds, and expanded network fault injection all green (run `46019191067`) |

## Explicit limitations

- The runtime is a heartbeat/reliability proof of concept, not a general application-data, audio, or video transport.
- UDP reports are not retransmitted; there is no ack-of-ack or report reorder buffer yet.
- A missed heartbeat is liveness evidence, not proof that a specific IP packet was dropped.
- No discovery, NAT traversal, relay, path migration, browser runtime, persistent identity store, certificate revocation, or automated key rotation.
- Physical-device offline-LAN validation has **NOT RUN**. The namespace test does not establish compatibility with real Wi-Fi access points, client isolation, VLANs, device firewalls, or WAN-disconnected LANs.
- No WebRTC comparative reliability claim or production security audit.

## Evidence gates

1. Compile, lint, unit tests, and documentation build.
2. Two-process mutual TLS, exact peer pins, and AEAD-authenticated UDP.
3. Linux veth/tc netem mirrored directional packet-drop experiment.
4. Real device/LAN tests with internet disabled and realistic Wi-Fi/AP/firewall conditions.
5. Reproducible WebRTC baseline comparison with absolute rates, sample sizes, and confidence intervals.

## Verified CI snapshot

Date: 2026-11-27  
Commit: `c02e6d0d855fa7481537ac2096726bff7c510714`  
GitHub Actions run: [`46019191067`](https://github.com/johnr1sh/Ares/actions/runs/46019191067) — both jobs passed.

- Rust formatting: passed.
- Shell syntax checks: passed.
- LAN credential provisioning smoke test: passed; A/B bundles have matching 32-byte session keys, mode 0600, and no CA signing key.
- Clippy with warnings denied: passed.
- Unit tests: **56 passed, 0 failed**.
- Native peer build and documentation build: passed.
- Real Linux namespace/veth test: passed all phases. A→B-only egress loss made A report A→B Failed / B→A Healthy. Mirrored B→A-only loss made B report B→A Failed / A→B Healthy; A conservatively downgraded its unverified opposite-direction evidence to Degraded. A simultaneous bidirectional blackhole left both aggregate sessions Degraded rather than fabricating OneWay or Unavailable. Restoring each impairment returned both endpoints to BidirectionalHealthy.
- Freshness/recovery regression tests: passed. Old positive and negative evidence expires; a stale direction cannot return to Healthy after only one new positive observation. The mutual-auth gate notification regression test also passed.

This validates the tested Linux namespace topology and impairment profiles only. It does not validate physical devices, Wi-Fi/AP isolation, VLAN/firewall behavior, WAN-disconnected LAN operation, broad long-duration impairment profiles, or a comparative WebRTC reliability ratio. The physical-device test remains **NOT RUN**.
