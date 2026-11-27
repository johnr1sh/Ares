# ARES validation and fault-injection plan

Status: unit-test coverage exists for the transport-independent core. No network benchmark or security validation result is claimed.

## Test record

Every run records source commit/build configuration; OS/device/CPU/memory/interfaces/topology; transport/protocol settings; impairment profile and deterministic seed; test timestamps and observation window; expected/observed probe counts; per-direction outcomes and failure classification; latency distributions, loss, reordering, duplication, queue depth, CPU and memory; harness version and artifact checksums.

Never store payloads, credentials or private keys in test artifacts.

## Current automated unit coverage

The Rust tests cover:
- Direction transition guards and saturating counters.
- Aggregate session classification, including one-way, closed, and incomplete opposite-direction evidence.
- Frame round-trip, empty payload, invalid magic/version/kind, reserved flags, truncated header, oversized payload, and declared/actual length mismatches.
- Replay acceptance, duplicates, bounded reordering, stale values, large sequence jumps, and the u64 boundary.
- Probe policy validation, bounded in-flight work, strictly increasing sequences, timeout/miss transitions, expired acknowledgements, terminal close, and recovery hysteresis.
- Receiver-reported delivery evidence: send submission is not delivery, unsolicited reports are rejected, missing reports degrade/fail only their direction, recovery requires consecutive receipts, pending work is bounded, and out-of-order receiver reports are rejected.
- Bidirectional delivery classification: one direction can be explicitly failed while the opposite direction remains confirmed healthy; one healthy direction alone never marks the session healthy; a failed direction plus an unverified reverse direction is Degraded, not falsely called OneWay; two confirmed failed directions are Unavailable.
- Replay-window exact 127/128 sequence-distance boundaries and integer overflow edges.
- Crate-level prohibition of unsafe Rust.

The repository CI runs formatting, Clippy with warnings denied, and all-target/all-feature tests. A commit is not considered validated until the corresponding Actions run is green.

## Required two-peer acceptance tests

1. Two peers authenticate using the selected trust bootstrap.
2. Unauthenticated or identity-substituted peers cannot establish an application session.
3. A_TO_B and B_TO_A probes are independently observed and acknowledged.
4. Blocking only A_TO_B yields ONE_WAY and identifies A_TO_B as failed.
5. Blocking only B_TO_A yields the mirrored result.
6. Blocking both directions yields UNAVAILABLE.
7. A successful transport handshake with application delivery blocked is not reported healthy.
8. A working LAN session survives loss of internet access.
9. An unavailable LAN path is reported accurately; offline mode cannot bypass client isolation or firewall policy.
10. Recovery obeys retry/time budgets and reports terminal failure.
11. Oversized, malformed, duplicate and replayed frames are rejected safely.
12. Diagnostics contain no payloads or secrets.

## Network impairment matrix

Direction-specific packet drop/firewall rules; random and burst loss; delay/jitter/reordering/duplication; bandwidth restriction and sudden collapse/recovery; MTU reduction; IPv4-only/IPv6-only/dual-stack; Wi-Fi client isolation, VLANs and multicast filtering; NAT/CGNAT/stateful firewall and later relay failure; interface switching; internet outage with LAN intact; full LAN outage; high/asymmetric RTT and asymmetric loss.

## Device and process matrix

Sleep/wake and background/foreground; process crash/restart; interface up/down; memory/CPU pressure and battery saver; later media permission denial/revocation; wall-clock jumps while monotonic timers remain correct.

## Test layers

Unit tests for state machines, framing, replay windows, timeouts and limits; property-based tests for invariants; fuzz tests for parsers and handshake/state transitions; two-process integration tests; network-namespace impairment tests; real-device LAN tests; soak/load/recovery tests; security negative tests; dependency audits.

Every bug fix includes a regression test that fails before the fix and passes afterward.

## Reliability statistics

For each direction and the joint bidirectional outcome, publish numerator, denominator and confidence interval. With zero failures in N independent trials, a rough 95% upper bound is 3/N; independence is often unrealistic, so stratify by device, network and impairment profile and use an appropriate statistical model.

A 100× improvement claim requires the same metric, baseline implementation/version, hardware, topology, impairment profile, duration and harness. Publish absolute rates and uncertainty, not only a ratio. If baseline failures are zero, a 100× reduction cannot be inferred.

## Core 0.1 release gate

Do not call Core 0.1 validated until mandatory acceptance tests pass on documented supported platforms; both one-way failure directions are detected/localized; offline LAN operation is demonstrated on a documented topology; resource/retry limits are exercised; security negative tests pass; and scripts/reproduction instructions are published.
