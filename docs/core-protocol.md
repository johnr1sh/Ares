# ARES Core 0.1 protocol requirements

Status: requirements draft. This is not a wire-format specification and is not yet implemented.

## Scope and threat model

Core 0.1 is a two-peer authenticated session over a viable IP network, with local mode independent of internet access. Attackers may observe, inject, modify, delay, duplicate, reorder or replay packets; impersonate peers; poison discovery; attempt downgrade; exploit malformed framing; or exhaust CPU, memory, sockets and queues. A LAN is not inherently trusted.

## Session establishment

1. Discover endpoint candidates without treating discovery as identity proof.
2. Authenticate peers before accepting application data.
3. Bind protocol version, peer identities, capabilities and session parameters into the authenticated handshake transcript.
4. Derive directional traffic keys using an established, reviewed cryptographic protocol/library. Do not design custom cryptography.
5. Reject replayed, unauthenticated, malformed or out-of-session data.
6. Fail closed on unsupported mandatory features or invalid authentication.
7. Support explicit close, timeout and crash/restart detection.
8. Keep secrets out of logs, crash reports, metrics and test artifacts.

The trust bootstrap (for example, verified QR fingerprint or short-lived pairing secret) requires an ADR. An unauthenticated public-key exchange is vulnerable to man-in-the-middle attacks.

## Directional health protocol

Each peer maintains independent directional health records. A probe carries a non-secret session identifier, direction, monotonically increasing sequence number, sender monotonic timestamp/sample, probe type and bounded payload length. Authentication is provided by the selected secure channel.

The receiver validates membership, sequence/replay window, size and authentication, records receipt, and emits an acknowledgement with explicit semantics. The sender records acknowledgement receipt.

A direction is not healthy merely because a probe was queued locally. Record forward delivery and reverse acknowledgement separately. Use monotonic clocks for local durations. Do not subtract wall-clock timestamps from different devices to claim one-way latency unless clock synchronization and uncertainty are measured. RTT does not require synchronized clocks.

## Delivery classes

### Reliable class
- Ordered delivery where requested.
- Explicit flow control and backpressure.
- Bounded buffering and message size.
- Cancellation and timeout semantics.
- Application acknowledgement only when transport delivery is insufficient.

### Deadline-bound class
- Explicit expiry/deadline.
- No retransmission promise after deadline.
- Late data is counted and discarded or surfaced by API policy.
- Loss, reordering, duplication and expiry are observable.
- Congestion control remains mandatory.

## Framing and parser safety

Specify a versioned frame format with unambiguous framing, message type, bounded length, session/stream association, sequence number where required, authenticated protection, and defined handling for unknown types/versions.

Reject overlong lengths before allocation. Fuzz parsers and state transitions. Never deserialize untrusted input into unsafe object graphs or invoke code based on remote type names.

## Failure classification

At minimum distinguish:
- LOCAL_SOURCE_UNAVAILABLE
- LOCAL_SEND_REJECTED
- REMOTE_RECEIPT_TIMEOUT
- AUTHENTICATION_FAILED
- REPLAY_REJECTED
- REMOTE_PARSE_FAILED
- REMOTE_APPLICATION_NOT_DELIVERING
- ACKNOWLEDGEMENT_TIMEOUT
- PATH_UNREACHABLE
- SESSION_EXPIRED
- RESOURCE_LIMIT_REACHED
- PROTOCOL_VERSION_UNSUPPORTED

Do not collapse all faults into disconnected. Diagnostics identify stage and direction without exposing payloads or secrets.

## Recovery requirements

Retry budgets and deadlines are bounded. Backoff prevents retry storms. Alternate paths are validated before replacing a working path. Duplicate in-flight operations are idempotent or deduplicated. Recovery does not reset identity or accept stale session data. Report terminal failure when the recovery budget expires.

## Decisions required before wire-format freeze

Implementation language and OS support; transport and datagram mechanism; peer identity and first-contact trust bootstrap; cryptographic protocol/library and key lifecycle; discovery; frame encoding and version negotiation; probe interval and thresholds; and whether reliable/deadline-bound classes share a connection. Resolve through ADRs and runnable experiments.
