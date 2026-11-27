# ARES architecture

Status: architecture proposal with a partially implemented, transport-independent Rust reliability core.

## Scope

The first deliverable is a native two-peer core that will prove authenticated communication over a viable IP network, including local-network operation without internet access, and independently detect/localize one-way failures.

The repository currently implements state, framing, replay-window bookkeeping, probe tracking, and a receiver-reported directional delivery ledger. It does not yet implement sockets, QUIC, authenticated sessions, discovery, offline-LAN operation, media, browser support, NAT traversal, or a 100× reliability improvement.

## Current crate boundaries

1. **Directional state:** independent A→B and B→A state records and derived session state.
2. **Frame codec:** bounded version-1 frame parser/encoder with explicit message kind, session identifier, sequence number, and maximum payload length.
3. **Replay window:** fixed-memory 128-sequence window. It must be updated only after cryptographic authentication.
4. **Probe tracker:** bounded outstanding-probe ledger, monotonic acknowledgement deadlines, miss thresholds, and consecutive-success recovery.
5. **Delivery evidence ledger:** tracks sender submissions separately from authenticated receiver-reported receipt or non-receipt. Missing local ACKs are not treated as proof of a failed direction.
6. **Bidirectional delivery supervisor:** keeps A→B and B→A ledgers separate and derives session health from receiver-side evidence.
7. **Future transport adapter:** owns actual sends/receives and maps authenticated remote reports to the reliability supervisor.
8. **Future identity/security layer:** pairing, mutual peer authentication, encrypted transport, authorization, replay enforcement, expiry and revocation.

## Non-negotiable invariants

### I1 — Directional health is independent

For peers A and B, track A_TO_B and B_TO_A separately. A session is bidirectionally healthy only when both required directions satisfy the health policy. A transport handshake or socket state is not proof of application delivery.

### I2 — Health evidence is stage-specific

Record source production, local send acceptance, remote receipt, authentication result, application delivery, and acknowledgement separately. A local send success is not end-to-end health. Probe/ack timeouts measure a request/response path; they do not by themselves prove which leg of that round trip dropped the packet.

### I3 — Control and data failure are separable

Failure of an optional data stream must not automatically destroy a healthy control channel. Internet loss must not terminate a session whose local path remains valid.

### I4 — Recovery and resource use are bounded

Every retry, queue, buffer, parser allocation, handshake, and timer has a documented limit. Recovery has a deadline and terminal failure reason. The current codec caps payloads at 64 KiB and the probe tracker bounds outstanding work.

### I5 — Security precedes trust

Discovery results are hints, not authenticated identities. No peer data is trusted before cryptographic authentication. No silent plaintext or weaker-suite fallback. Current protocol framing is not security protection.

### I6 — Delivery semantics are explicit

Each stream declares reliable/ordered or time-sensitive/deadline-bound behavior. Expired time-sensitive data is dropped rather than silently delivered late.

## Candidate transport strategy

Evaluate mature options before inventing a transport:
- QUIC streams for reliable ordered control/data.
- QUIC DATAGRAM or another bounded datagram path for deadline-bound data.
- WebTransport for browser environments where supported.
- Native UDP only where platform access and security design justify it.

QUIC alone does not provide local discovery, NAT traversal, codecs/capture, group key management, or browser raw-socket access. Quinn remains a candidate until a runnable authenticated prototype and fault-injection tests validate it.

## Local-first connection flow

1. Discover or manually enter a peer endpoint on the local network.
2. Treat the endpoint as untrusted; authenticate using a separately specified trust bootstrap.
3. Negotiate protocol version and capabilities with downgrade protection.
4. Establish encrypted transport and bind authenticated identity to the handshake transcript.
5. Run independent A_TO_B and B_TO_A application-level probes and collect endpoint-side receipt evidence.
6. Declare usability only according to requested directionality and stream policy.
7. Continue health probes and privacy-safe telemetry.
8. Validate an alternate path before switching; do not discard a working path prematurely.

Multicast, broadcast, mDNS, QR and manual pairing are candidates, not assumptions. Wi-Fi client isolation, VLANs, host firewalls, multicast filtering and OS permissions can prevent same-LAN reachability.

## Browser boundary

Ordinary web pages do not have general-purpose raw UDP socket access. A browser runtime must use supported APIs such as WebTransport where available, a local companion/gateway, or another justified standards-based approach. Native/browser parity must be tested, not promised.

## Reliability measurements

Let S_AB and S_BA denote successful end-to-end delivery in each direction over a defined interval.

- Bidirectional success: P(S_AB AND S_BA); measure jointly rather than assuming independence.
- One-way failure: P(S_AB XOR S_BA).
- Failure improvement: baseline_failure_rate / ARES_failure_rate, for identical metric definitions and conditions.
- Report absolute rates, median, p95, p99, confidence intervals, sample sizes and topology/device strata.

A 100× ratio is not meaningful if the baseline has zero observed failures or test conditions differ.

## ADR policy

Decisions about transport, crypto suite, wire compatibility, discovery or browser gateway require an ADR with context, constraints, options, evidence, security/performance implications, decision and reversal conditions.
