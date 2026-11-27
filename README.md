# ARES

**ARES** is an open-development project for real-time audio, video, messaging, and data communication, designed around offline-capable networking and independently verified communication directions.

Its first target is **ARES Core 0.1**: a two-peer native foundation that will establish authenticated sessions over viable IP networks, including local networks without internet access, and detect/localize one-way communication failures.

> Reliability is an engineering objective, not a current performance claim. A target of 100× fewer failures than a named WebRTC baseline must be demonstrated by reproducible, statistically defensible tests before it is claimed.

## Current implementation

The Rust crate now contains reliability primitives and an early native two-peer runtime:

- Independent A→B and B→A health records with a derived session state.
- A bounded, versioned application-frame encoder/decoder.
- A fixed-memory 128-sequence replay window.
- A monotonic probe/ack tracker with acknowledgement deadlines, bounded in-flight work, miss thresholds, and consecutive-success recovery.
- A receiver-reported delivery ledger that keeps A→B and B→A independent and bounds unconfirmed receipts.
- A bidirectional delivery supervisor with direct receiver observations and authenticated remote receipt evidence.
- A native peer process with mutual TLS identity validation, exact SHA-256 leaf pins, and independent AEAD-protected UDP heartbeat/report traffic.
- A Linux network-namespace fault-injection suite that tests both one-way loss directions, bidirectional blackhole ambiguity, recovery, and stale-evidence hysteresis using real tc netem packet drops.
- A physical-device offline-LAN provisioning helper, peer launcher, and step-by-step WAN-disconnection test guide.

The native runtime currently proves heartbeat delivery and directional health; it is not yet a general application-data, audio, or video transport. Its UDP heartbeat and pre-shared-key design is a proof of concept, not the final transport decision. The latest full CI run passed on commit `c02e6d0` (run `46019191067`): formatting, shell checks, LAN provisioning smoke test, Clippy, 56 unit tests, native build, docs, and the expanded network-layer experiment all passed. The network test covered A→B-only loss, mirrored B→A-only loss, a bidirectional blackhole, stale-evidence handling, and recovery. CI uses Linux namespaces and veth; this is not physical Wi-Fi/LAN validation. This is one controlled Linux topology, not a physical Wi-Fi/LAN or WebRTC performance claim. The real-device WAN-disconnection test remains **NOT RUN**; it requires two physical endpoints and their logs. The frame codec is not cryptographic protection; UDP frames are authenticated and encrypted with AEAD after mutual TLS peer authentication.

See [implementation status](docs/implementation-status.md) for the exact boundary between working code and planned work.

## Non-negotiable properties

- **Bidirectional correctness:** A→B and B→A are independently measured. “Connected” never implies both directions work.
- **Offline-first:** local sessions must work over a viable LAN without cloud or internet dependencies.
- **Secure by default:** authenticate peers, encrypt traffic, reject replay, bound resource use, and never silently downgrade.
- **Failure isolation:** control, media, and data subsystems have independent health and bounded recovery.
- **Evidence-led reliability:** every claim names its baseline, metric, environment, sample size, and uncertainty.
- **Graceful degradation:** preserve working capabilities when another path or subsystem fails.

## Roadmap

1. **M0 — Foundation:** requirements, threat model, architecture, protocol record, CI, and fault-injection plan. The documentation and initial crate foundations are present.
2. **M1a — Reliability primitives:** framing, replay-window bookkeeping, directional state, and probe lifecycle. Implemented in the Rust crate; CI verification is required for each commit.
3. **M1b — Native transport proof:** mutual TLS, pinned peer identity, two-process heartbeat runtime, and directional packet-drop experiment. QUIC remains a candidate for a later transport decision.
4. **M1c — Local connectivity:** manual/QR pairing, endpoint selection, offline-LAN tests, and direction-specific network impairment.
5. **M2 — Media:** audio/video pipeline, clocking, jitter handling, synchronization, and adaptive quality.
6. **M3 — Connectivity:** IPv4/IPv6, NAT traversal, relay policy, interface/path migration, and recovery.
7. **M4 — Runtimes:** browser-compatible transport/API with explicit capability negotiation.
8. **M5 — Multiparty:** group security, forwarding, congestion/load control, and scale testing.

## Documentation

- [Implementation status](docs/implementation-status.md)
- [Native runtime and packet-drop test](docs/native-runtime.md)
- [Real-device offline-LAN test](docs/real-device-lan-test.md)
- [Architecture](docs/architecture.md)
- [Core protocol and frame format](docs/core-protocol.md)
- [Security model](SECURITY.md)
- [Validation and fault-injection plan](docs/testing.md)
- [Contributing](CONTRIBUTING.md)

## License

No license has been selected or published yet. Until a license file is added, all rights are reserved by default; do not assume the repository is already licensed for reuse.
