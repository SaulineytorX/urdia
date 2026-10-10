# ADR 0024: Human-confirmed one-off profile execution

Status: Accepted (experimental)
Date: 2026-10-02

## Context

The dashboard can recommend declared execution profiles but previously required
the operator to leave the UI to launch one. ADR 0015 still prohibits automatic
routing; passing a recommendation is not proof of task quality or authorization.

## Decision

Add a separate host-controlled review/confirm flow for the latest successful
profile recommendation. The host requires a dedicated available launch, exact
model selector and an effort selector when an effort label is configured. Bare
agent recommendations and label-only profiles remain advisory.

Preparing a review starts no agent or inference. A memory-only random ticket
binds the original retained objective, immutable profile/launch configuration,
workspace and permission mode. It expires after five minutes; a new recommendation
invalidates an unaccepted ticket. At most one ticket exists per bounded advisory
record. Browser input contains only opaque IDs, never a target, prompt, workspace,
controls or permissions. Authentication precedes parsing.

Confirmation queues exactly one turn. Successful confirmation is idempotent for
the same ticket and turn ID, including after task failure/cancellation. A different
turn ID conflicts. A busy/closed/full conversation queue does not consume the
ticket. Conversation idempotency includes both task text and target profile, so
ordinary prompt IDs cannot be reused to dispatch another agent.

The CLI starts a fresh session for that one task using the host-owned workspace,
exact configured controls, existing durable receipt sink and one-shot broker (or
deny-by-default). It rechecks executable availability and both startup selectors
and current explicit-control agreement before sending the objective. Startup is
bounded to 45 seconds and is cancellable. No implicit fallback or provider/model
substitution occurs. Native provider errors stay redacted in the task UI.

Each successful startup has its own session history and closes after the turn.
Per-turn identity and reported selectors are separate from the primary conversation
context. The dashboard shows the selected task identity while active, then restores
the default for subsequent ordinary prompts. Existing timings/tool reports apply;
model-reported tests are not independent host verification.

This initial implementation disables the flow in hosts started with
`--trusted-session`, even before trust activation. It does not inherit a trusted
scope, select persistent ACP approval or grant a permission. Per-operation controls
remain independent. Agent execution may consume provider quota.

## Consequences and verification

Automatic routing and frozen evaluation artifacts remain unchanged. This is
operator-approved execution, not activation of the automatic orchestration gate.
Agent reports do not attest remote inference, credentials or comparative quality.
Catalog editing, persistence across restart, multi-task planning/concurrency,
automatic effort selection and broader dashboard settings remain future work.

Offline tests cover no dispatch before confirmation, replay/conflict isolation,
expiry, stale/fallback choices, authentication and input tampering, exact-pin
eligibility, pre-start cancellation and mismatch-before-prompt. Browser acceptance
uses a local Laya fixture and a different dedicated ACP profile, rejects a tool
request, verifies identity/metrics and confirms the next ordinary task still uses
the primary profile. No account inference is part of ordinary CI.

ADR 0030 extends this executor to manually selected next-task defaults from the
already loaded host catalog. One-off review tickets still retain their explicit
immutable targets and do not change that manual default.
