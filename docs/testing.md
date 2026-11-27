# ARES validation and fault-injection plan

Status: test strategy. No benchmark results are claimed.

## Test record

Every run records source commit/build configuration; OS/device/CPU/memory/interfaces/topology; transport/protocol settings; impairment profile and deterministic seed; test timestamps and observation window; expected/observed probe counts; per-direction outcomes and failure classification; latency distributions, loss, reordering, duplication, queue depth, CPU and memory; harness version and artifact checksums.

Never store payloads, credentials or private keys in test artifacts.

## Core acceptance tests

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

Unit tests for state machines, counters, sequence windows, timeouts and limits; property-based tests for invariants; fuzz tests for parsers and handshake/state transitions; two-process integration tests; network-namespace impairment tests; real-device LAN tests; soak/load/recovery tests; security negative tests; dependency audits.

Every bug fix includes a regression test that fails before the fix and passes afterward.

## Reliability statistics

For each direction and the joint bidirectional outcome, publish numerator, denominator and confidence interval. With zero failures in N independent trials, a rough 95% upper bound is 3/N; independence is often unrealistic, so stratify by device, network and impairment profile and use an appropriate statistical model.

A 100x improvement claim requires the same metric, baseline implementation/version, hardware, topology, impairment profile, duration and harness. Publish absolute rates and uncertainty, not only a ratio. If baseline failures are zero, a 100x reduction cannot be inferred.

## Core 0.1 release gate

Do not call Core 0.1 validated until mandatory acceptance tests pass on documented supported platforms; both one-way failure directions are detected/localized; offline LAN operation is demonstrated on a documented topology; resource/retry limits are exercised; security negative tests pass; and scripts/reproduction instructions are published.
