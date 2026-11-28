# ARES Core 0.1 protocol

Status: version-1 frame codec implemented; secure session and transport are not implemented. This is the current framing contract, not a complete secure wire protocol.

## Security boundary

The frame codec only parses and serializes bounded bytes. It provides no encryption, integrity, sender authentication, identity binding, authorization, or replay enforcement. Frames must travel inside a separately authenticated and encrypted channel before fields or payloads are trusted. Update replay state only after authentication.

## Version-1 frame format

All integer fields are unsigned big-endian. The fixed header is 36 bytes.

| Offset | Length | Field | Rule |
|---:|---:|---|---|
| 0 | 4 | Magic | ASCII `ARES` |
| 4 | 1 | Version | `1` |
| 5 | 1 | Message kind | `1` Probe, `2` Acknowledgement, `3` Data |
| 6 | 2 | Flags | Must be zero in version 1 |
| 8 | 16 | Session identifier | Opaque, non-secret session value |
| 24 | 8 | Sequence | Monotonic per sending stream |
| 32 | 4 | Payload length | Must exactly match remaining bytes |
| 36 | variable | Payload | Maximum 65,536 bytes |

The decoder rejects truncated headers, wrong magic, unsupported versions, unknown message kinds, non-zero reserved flags, oversized payloads, and declared/actual length mismatches. It borrows the payload slice and does not allocate while decoding. The encoder allocates at most one frame bounded by the same payload limit.

A 64 KiB frame limit is a parser/resource bound, not a recommendation to send 64 KiB QUIC DATAGRAMs. Datagram payload sizing must be negotiated and constrained by path MTU; larger reliable data belongs on a stream.

## Replay window

The crate provides a fixed-memory 128-sequence sliding replay window. It accepts reordered messages inside the window once, rejects duplicates, and rejects values older than the window. The caller must authenticate first and scope a replay window to the correct peer/session/stream context.

## Probe lifecycle

The probe tracker:
- Requires strictly increasing sequence numbers.
- Bounds outstanding probes.
- Uses monotonic `Instant` deadlines.
- Expires each outstanding probe only once.
- Moves through Starting, Degraded, Failed, Recovering, and Healthy according to configured thresholds.
- Requires consecutive successful acknowledgements to recover from Failed.
- Rejects unknown/expired acknowledgements and makes Closed terminal.

The tracker models an acknowledgement exchange (a request/response path). An ACK timeout alone cannot distinguish a lost forward probe from a lost reverse ACK. A real two-peer supervisor must combine both endpoints' directional send/receipt observations and separately measured probe directions before asserting which physical direction failed.

## Receiver-reported directional delivery evidence

The crate provides a bounded delivery ledger for each direction and a bidirectional supervisor. A local send accepted by a transport is recorded as pending; it does not mark the direction healthy. Only a receiver-originated report that is authenticated, bound to the session, and bound to the correct direction can confirm delivery.

A receiver may report a sequence missing only after its own observation deadline. A sender-side ACK timeout is not sufficient evidence to call a physical direction failed: the forward probe or the reverse receipt may have been lost. The receiver's authenticated report provides the missing directional observation when the report path is available.

The two ledgers remain independent. A session is classified OneWay only when one direction has explicit receiver-reported missing evidence beyond its configured threshold while the opposite direction has confirmed receiver-reported delivery. If the reverse reporting path is also unavailable, the result remains incomplete/degraded rather than inventing a direction diagnosis.

The ledger bounds pending receipts (hard maximum 4,096 per direction), requires increasing send sequence numbers, rejects unsolicited/duplicate reports, and buffers authenticated reports that arrive out of order. Buffered reports remain within the existing pending bound and are applied in original probe order, so loss and recovery thresholds cannot be reordered by network arrival. A gap in reports intentionally blocks later outcomes from being applied until the earlier report arrives; this avoids false health transitions but means report retransmission is still needed to unblock progress after a permanently lost report. The integration must authenticate each report and enforce its session/direction binding before invoking the API. The native proof-of-concept runtime sends reports as ChaCha20-Poly1305-protected UDP datagrams, with keys derived by HKDF-SHA-256 from an out-of-band session PSK and separated by direction/message kind. The reusable ledger crate itself does not perform cryptographic authentication. The adapter still has no report retransmission or ack-of-ack protocol.

## Session health

A session is bidirectionally healthy only when both direction records are Healthy. One Healthy direction plus a confirmed Failed/Closed opposite direction is OneWay. A Healthy direction paired with Unknown, Starting, Degraded, or Recovering evidence is Degraded, never BidirectionalHealthy.

## Still required

- Production pairing/first-contact trust bootstrap and transcript-bound capability negotiation.
- Production key rotation/revocation and a formal session-key lifecycle.
- UDP report retransmission/ack-of-ack and stronger recovery semantics.
- QUIC/media transport evaluation, path selection and teardown.
- Explicit reliable and deadline-bound delivery APIs.
- Discovery, offline-LAN proof, NAT/firewall behavior and browser runtime.
- Fuzzing, two-process integration, fault injection and external security review.
