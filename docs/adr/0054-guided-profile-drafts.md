# ADR 0054: Guided profile drafts with manual ownership

Status: Accepted
Date: 2026-10-15

## Decision

Keep registered launch selection, explicit model/effort inspection and the
profile name visible; disclose exact IDs and display metadata under Advanced.
Validation opens that section before focusing missing fields. The manual path
remains available when an agent cannot report controls.

Only new drafts receive deterministic local ID/name/agent suggestions. IDs obey
the host grammar, resolve collisions within a bounded search and remain within
128 bytes. Generated labels are bounded by UTF-8 bytes and omit control
characters. An input event establishes manual ownership, including an empty
value; later suggestions never overwrite it. Existing profiles and drafts
survive polling without suggestion-driven replacement. Suggestions use inert
text and never infer credentials, inference vendors or entitlement.

## Boundaries

ADR0025/0027 inspection consent, stale result scope, retries, exact selectors,
dedicated launches, explicit saves and next-launch application stay intact.
There are no wire/API/storage changes, task dispatches, permission changes or
model calls for suggestions. Discovery remains an optional agent startup.

## Verification

Node regressions exercise collision boundaries, UTF-8 limits, missing inputs and
manual ownership. Offline ACP browser acceptance at desktop and mobile widths
checks discovery, manual inputs/deletions, hidden-field focus, host saves and
existing drafts. These fixtures do not validate any remote model or account.
