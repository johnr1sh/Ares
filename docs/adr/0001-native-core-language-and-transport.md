# ADR 0001: Native Core language and transport candidate

- Status: proposed; not accepted
- Date: 2026-11-27
- Decision owners: ARES maintainers

## Context

ARES Core 0.1 needs a native implementation that can run on common desktop/server platforms, support authenticated bidirectional sessions, and carry both reliable ordered data and deadline-bound data. The project must not prematurely create a custom transport.

## Constraints

- Local-network sessions must not require an internet rendezvous service.
- Peer identity must be authenticated; encryption alone is not sufficient.
- Directional health must be measured at application level.
- Reliable and deadline-bound delivery semantics must be explicit.
- Congestion control, MTU constraints, bounded memory and parser safety are mandatory.
- Browser support is a separate runtime concern and cannot assume raw UDP access.

## Options considered

1. Rust with Quinn QUIC and rustls-backed TLS.
2. Rust with a different mature QUIC implementation.
3. A custom UDP protocol.
4. Start with TCP/TLS only and add a datagram path later.

## Evidence

- Quinn describes itself as a pure-Rust, async-compatible IETF QUIC implementation. Its documented feature set includes simultaneous client/server operation, reliable streams, application datagrams, and stable Rust support on Linux, macOS and Windows: https://github.com/quinn-rs/quinn
- Quinn API documentation describes a portable userspace QUIC implementation and endpoint operation suitable for peer-to-peer applications: https://docs.rs/quinn/latest/quinn/
- RFC 9221 defines unreliable QUIC DATAGRAM frames. They are not retransmitted, are congestion-controlled, and cannot be fragmented; the application must account for the effective datagram size and path MTU: https://www.rfc-editor.org/rfc/rfc9221
- rustls is a maintained Rust TLS implementation with TLS 1.2 and 1.3 support: https://github.com/rustls/rustls
- WebTransport exposes reliable streams and unreliable datagrams to browser applications but remains a browser API with a client/server HTTP/3 model: https://developer.mozilla.org/en-US/docs/Web/API/WebTransport_API

## Proposed direction

Use Rust for the first native Core implementation and evaluate Quinn as the QUIC engine. Use the TLS integration supported by the selected Quinn version. Do not add a custom cipher, congestion controller, or transport protocol.

Keep transport behind an internal interface so that a different implementation can be substituted if experiments reveal a concrete limitation.

Do not treat a successful QUIC connection as proof of authenticated ARES peer identity until the selected certificate/pairing trust bootstrap is designed and tested. First-contact identity binding remains a separate mandatory decision.

## Consequences and known limitations

- QUIC streams can carry reliable ordered control and data.
- QUIC DATAGRAM can carry deadline-bound data, but loss is not retransmitted and maximum size is constrained by negotiated limits and path MTU.
- QUIC does not solve discovery, NAT traversal, firewall policy, relay selection, group key management, or browser-to-browser connectivity.
- Rust/Quinn availability on desktop does not prove support on every mobile target or browser runtime.
- A LAN-only prototype must be tested with internet disconnected and with realistic Wi-Fi isolation/firewall scenarios.

## Validation required before acceptance

1. Build a two-peer native prototype using the selected versions.
2. Demonstrate simultaneous peer operation on a documented LAN topology.
3. Authenticate peer identity using a separately reviewed pairing/certificate design.
4. Demonstrate reliable stream delivery and deadline-bound datagram behavior.
5. Block A-to-B only, then B-to-A only; verify independent detection and classification.
6. Exercise MTU reduction, packet loss, reordering, duplication, high RTT and resource limits.
7. Verify that the session operates with internet access disabled when the LAN path remains viable.
8. Record exact dependency versions, platform matrix, test commands and results.

## Reversal conditions

Reconsider Quinn if the prototype cannot meet required platform support, LAN operation, security integration, datagram behavior, or measurable performance goals, or if a better-maintained implementation provides materially stronger evidence.
