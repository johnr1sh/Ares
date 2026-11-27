# Contributing to ARES

ARES is an experimental communication-systems project. Reliability and security claims require evidence.

## Before changing code

Read the architecture, Core protocol requirements, security model and testing plan. State the observable failure mode. Determine whether the behavior is a protocol invariant, implementation detail or platform constraint. Do not commit to transport or cryptographic designs without an ADR and reproducible evidence.

## Pull requests

Include the problem and scope; design and alternatives; security/privacy impact; tests added and exact commands/results; performance measurements where relevant; platform limitations and known risks; and documentation/migration implications.

Do not claim tests passed unless actually run. Do not claim security review, browser support, offline support or reliability improvement without evidence.

## Engineering rules

Keep directional health independent. Treat discovery as untrusted. Use established cryptographic libraries. Bound queues, allocations, retries and parsing. Add regression tests for fixed defects. Keep diagnostics free of secrets and payloads. Prefer small, reviewable changes and explicit interfaces.
