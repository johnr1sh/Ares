# ARES

**ARES** is an open-source, offline-capable real-time communication system for audio, video, messaging, and data.

Its first engineering target is **ARES Core 0.1**: a secure two-peer prototype that can establish a local session without internet, independently verify both communication directions, and detect/localize one-way failures.

> Reliability is an engineering objective, not a current performance claim. A target of 100× fewer failures than a named WebRTC baseline must be demonstrated by reproducible, statistically defensible tests before it is claimed.

## Project status

The repository is at the requirements and architecture foundation stage. The current work establishes invariants, threat boundaries, protocol requirements, and a reproducible validation plan before committing to a transport or implementation language.

## Non-negotiable properties

- **Bidirectional correctness:** A→B and B→A are independently measured. “Connected” never implies that both directions work.
- **Offline-first:** peers can communicate over a viable local network without a cloud service or internet connection.
- **Secure by default:** authenticate peers, encrypt traffic, reject replay, bound resource use, and never silently downgrade.
- **Failure isolation:** control, media, and data subsystems have independent health and recovery.
- **Evidence-led reliability:** every reliability claim names its baseline, metric, environment, sample size, and uncertainty.
- **Graceful degradation:** preserve working capabilities when another path or subsystem fails.

## Engineering roadmap

1. **M0 — Foundation:** requirements, threat model, architecture, protocol decision records, benchmark and fault-injection plan.
2. **M1 — Core 0.1:** authenticated two-peer sessions, bidirectional probes, reliable and deadline-bound data, LAN-only operation, failure localization, bounded recovery.
3. **M2 — Media:** audio/video pipeline, clocking, jitter handling, synchronization, adaptive quality.
4. **M3 — Connectivity:** discovery, IPv4/IPv6, NAT traversal, relays, interface/path migration.
5. **M4 — Runtimes:** native and browser-compatible implementations with explicit capability negotiation.
6. **M5 — Multiparty:** group security, forwarding, load control, and scale testing.

## Documentation

- [Architecture](docs/architecture.md)
- [Core protocol requirements](docs/core-protocol.md)
- [Security model](SECURITY.md)
- [Validation and fault-injection plan](docs/testing.md)
- [Contributing](CONTRIBUTING.md)

## Design discipline

The documents define requirements and candidate designs; they do not imply that a transport, cryptographic suite, or implementation has already been selected or validated. Decisions that affect interoperability or security must be recorded as ADRs with evidence and reversal conditions.

## License

No license has been selected yet. Until a license file is added, all rights are reserved by default; do not assume this repository is already licensed for reuse.
