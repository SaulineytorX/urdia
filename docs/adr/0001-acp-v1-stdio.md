# ADR 0001: ACP v1 over stdio

- Status: accepted
- Date: 2026-09-25

## Decision

Urdia uses stable Agent Client Protocol v1 over stdio for local coding-agent
adapters. The wire layer is isolated in `urdia-acp`.

## Rationale

ACP v1 supports initialization, sessions, streaming updates, cancellation, and
permission requests across compatible agents. Stdio avoids opening a listening
port and follows the current stable local transport. ACP v2 remains a draft and
is excluded from the MVP.

## Consequences

Provider CLIs retain their own internal loops and authentication. Urdia maps
their events into its domain model and must preserve capabilities it cannot
normalize. A future protocol upgrade stays behind the ACP crate boundary.

