# Security policy and threat model

## Status

ARES is at the architecture/requirements stage. This is not a security certification, audit report, or claim that a secure implementation exists.

## Reporting vulnerabilities

Do not include exploit details, credentials, private keys or sensitive personal data in public issues. Use GitHub private vulnerability reporting if enabled; otherwise contact maintainers through a verified private channel.

## Assets and adversary capabilities

Assets include peer identity and pairing credentials, session secrets and traffic keys, messages and future media, membership/authorization state, device/network metadata, availability and update/protocol integrity.

Assume attackers can observe, modify, delay, inject, duplicate, reorder and replay traffic; impersonate endpoints; poison local discovery; attempt downgrade; send malformed frames; exhaust resources; and operate malicious devices on the same LAN. Relays and rendezvous services, if introduced, are not trusted with plaintext.

## Security requirements

1. Authenticate peer identity; discovery is not authentication.
2. Use established cryptographic protocols and maintained libraries. No custom cipher or home-grown key exchange.
3. Encrypt and authenticate sensitive application traffic.
4. Bind identities, protocol version, capabilities and session parameters to the handshake.
5. Reject replay, downgrade, invalid tags, malformed frames and stale credentials.
6. Enforce application authorization, not merely transport connection checks.
7. Bound message sizes, allocations, queues, concurrent handshakes, expensive work and retries.
8. Rotate and revoke credentials; define key erasure and session expiry.
9. Minimize metadata and redact secrets/payloads from diagnostics.
10. Treat LANs, discovery, relays and browser gateways as untrusted boundaries.
11. Pin dependencies, scan vulnerabilities and publish reviewed release artifacts.
12. Obtain independent security review before production security claims.

## Privacy-safe diagnostics

Permitted by default: coarse timestamps, direction, byte/packet counts, latency distributions, loss/late counts, protocol error codes and opaque short-lived session identifiers.

Forbidden by default: message/media payloads, pairing secrets, private keys, access tokens, full local address inventories, contact lists and persistent user/device identifiers.

Diagnostic export must be opt-in, inspectable, redacted and documented.

## Security testing

Include malformed-frame fuzzing, replay/duplicate tests, downgrade attempts, invalid authentication tags, identity substitution, discovery spoofing, resource exhaustion, key expiry/revocation, dependency audits, and negative tests proving unauthenticated peers cannot deliver application data.
