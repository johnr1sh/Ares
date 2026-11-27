# ADR 0001: Native Core language and transport candidate

- Status: Rust selected for the native Core; Quinn remains a transport candidate pending a runnable authenticated prototype
- Date: 2026-11-27
- Decision owners: ARES maintainers

## Context

ARES Core needs a native implementation that can run on common desktop/server platforms, support authenticated bidirectional sessions, and carry both reliable ordered data and deadline-bound data. The project must not prematurely create a custom transport.

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

## Decision

Use Rust for the native ARES Core. Keep the protocol engine independent of the transport, and require exact dependency versions plus a buildable two-peer proof before accepting a transport choice.

Quinn remains the leading candidate, not an implemented or finally accepted transport. Use its supported TLS integration if the prototype validates its platform support, peer identity model, LAN behavior, datagram limits, and operational complexity. Do not add a custom cipher, congestion controller, or transport protocol.

A successful QUIC connection alone is not proof of authenticated ARES peer identity. The first-contact trust bootstrap and peer identity binding must be designed and tested separately.

## Consequences and known limitations

- Rust provides one native implementation language for the first Core milestones.
- The frame codec, replay window, and probe tracker remain transport-independent.
- No transport, cryptographic suite, pairing scheme, discovery method, or browser gateway is frozen by this decision.
- QUIC does not solve discovery, NAT traversal, firewall policy, relay selection, group key management, or browser raw-socket access.
- Rust/Quinn availability on desktop does not prove support on every mobile target or browser runtime.
- A LAN-only prototype must be tested with internet disconnected and with realistic Wi-Fi isolation/firewall scenarios.

## Validation required before accepting a transport

1. Build a two-peer native prototype using exact, pinned dependency versions.
2. Demonstrate simultaneous peer operation on a documented LAN topology.
3. Authenticate peer identity using a separately reviewed pairing/certificate design.
4. Demonstrate reliable stream delivery and deadline-bound datagram behavior.
5. Block A-to-B only, then B-to-A only; verify independent detection and classification.
6. Exercise MTU reduction, packet loss, reordering, duplication, high RTT and resource limits.
7. Verify that the session operates with internet access disabled when the LAN path remains viable.
8. Record exact dependency versions, platform matrix, test commands and results.

## Reversal conditions

Reconsider Quinn if the prototype cannot meet required platform support, LAN operation, security integration, datagram behavior, or measurable performance goals, or if a better-maintained implementation provides materially stronger evidence.
