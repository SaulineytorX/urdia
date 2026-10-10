# ADR 0029: Guided workspace setup

Status: Accepted
Date: 2026-10-02

## Decision

Add an authenticated Setup guide with project, agent, execution profile,
decision engine and explicit test steps. Moving the existing form nodes reuses
validation, handlers, confirmations and drafts instead of adding an alternate
configuration API. Leaving the guide restores their original settings locations.
Step navigation has no network mutation or task side effect.

The project remains the host-bound workspace; the guide does not select arbitrary
filesystem paths. Agent installation/login and Jev credentials remain host-owned.
Laya reuses its existing session-only loopback connection form. Advice remains
separate from execution and permissions; ADR 0015 remains in force.

The final test requires an explicit checkbox and sends one arithmetic prompt to
the active coding agent through the existing authenticated conversation API.
It can consume account quota. It asks for no files/tools, but is not a sandbox.
Busy hosts, pending agent/profile restart changes, active trusted access and
unconfirmed live pins block submission. Permissions remain in Conversation.
Uncertain delivery is retried with the same turn UUID; no automatic retry or new
turn is generated. A completed response equal to `20` after trimming passes only
the arithmetic smoke check. Other responses need review. Metrics are host
observations for that exact test turn, never invented model/cost claims.

## Consequences

No new backend endpoint, credential persistence, active profile switching,
automatic routing or service installation is introduced. Progress/test identity
are in-memory per page, not a persistent installation certificate. Reloading
starts a fresh guide; conversation history still records the original turn.
Saved setup applies after the single restart defined by ADR 0028. A WSL test
with the owner's actual agents remains necessary before release.
