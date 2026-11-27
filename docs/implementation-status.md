# ARES implementation status

Last updated: 2026-11-27.

This page separates working code from design intent. It is updated with the code, not used as a roadmap substitute.

## Implemented in the Rust crate

| Area | Status | Boundary |
|---|---|---|
| Direction state | Implemented | Explicit state transitions and saturating counters |
| Session state | Implemented | Derived from two independent direction records; conservative when evidence is incomplete |
| Frame codec | Implemented | Version-1 bounded encoder/decoder; no authentication or encryption |
| Replay window | Implemented | Fixed 128-sequence window; caller must authenticate before updating |
| Probe tracker | Implemented | Monotonic deadlines, 1,024 hard maximum in-flight probes, miss thresholds, recovery hysteresis |
| Delivery evidence | Implemented | Per-direction bounded receipt ledger; only authenticated receiver reports count as delivery/missing evidence |
| Bidirectional delivery | Implemented | Combines independent directional ledgers; reports OneWay only with positive evidence for the healthy direction and explicit missing reports for the failed direction |
| CI | Configured | rustfmt, Clippy with warnings denied, and cargo tests |

## Not implemented

- Socket or QUIC transport.
- Mutual peer authentication, pairing, identity persistence, or encrypted application sessions.
- Session establishment/teardown across two processes.
- Reliable-stream or deadline-bound datagram APIs.
- Network transport for remote receipt telemetry; the current ledger API requires an authenticated integration layer to supply receiver reports.
- LAN discovery, offline-LAN verification, NAT traversal, relay selection, or path migration.
- Audio/video capture, codecs, rendering, synchronization, or browser runtime.
- Benchmark evidence, WebRTC baseline comparison, production readiness, or security audit.

## Engineering gates

1. **Gate A — compile and unit tests:** current primitives must pass formatting, lint, unit, and documentation tests in CI.
2. **Gate B — authenticated transport:** select and pin a maintained transport/TLS implementation; prove mutual peer authentication and encrypted two-process data exchange.
3. **Gate C — directional proof:** add endpoint receipt reporting and deterministic A→B-only, B→A-only, and bidirectional fault injection.
4. **Gate D — offline LAN:** demonstrate session setup and continued communication with internet disabled on documented devices and topology.
5. **Gate E — reliability claim:** compare against a pinned WebRTC baseline using identical hardware, topology, impairment, workload, and observation windows. Publish absolute rates and confidence intervals.

No later gate is considered passed because an earlier gate passed.
