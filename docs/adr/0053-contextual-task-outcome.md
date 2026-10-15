# ADR 0053: A contextual outcome after each task

- Status: accepted for source preview
- Date: 2026-10-15

## Context

Conversation has useful execution, verification and static-preview controls, but
separate panels above the transcript make the last response and next action hard
to find. A task finishing is different from passing independent checks or having
a supported web result. A simpler composition must preserve those distinctions.

## Decision

Bring the latest task's outcome next to its response and the next-task composer.
Show a concise agent state and a separate check state. Keep checks and preview
controls contextual, using their existing host-backed forms and handlers; disclose
technical detail progressively rather than duplicating forms or permissions.

No finished task means no claim that a result is ready. Failed or cancelled turns
must not receive a success headline. An earlier task's checks or preview never
certify the latest turn. A check-pass label requires a matching, non-stale job,
a successful host report and unchanged files during its check scope. It is not
proof that no files changed afterward or that every requirement is satisfied.

Opening controls or requesting a change must not send a task, run tests, prepare
a capture, contact a model, or grant a tool permission. Requesting a change focuses
the composer and preserves any draft. Check execution and browser-code preview
keep their separate, renewed consent. The preview link is opened explicitly in a
separate tab with its existing origin/token/expiry protections (ADR0052).

Projection and lease expiry must update even when the transcript sequence does
not change. Existing request IDs, delivery recovery, busy guards, cancellation,
agent selection and host authorization remain authoritative. Client-side labels
must not infer filesystem eligibility, invent model/cost evidence, or treat
opening a control as authorization.

## Consequences

Users can read a response, inspect its outcome and choose their next action in
one place. Expert details remain available. This UI reorganization introduces no
new host API, workflow autonomy, project inspection, dependency or storage policy.
An earlier live preview remains manageable after a failed/cancelled turn; while
a new task is active, the task rail exposes the existing same-ID stop action.
Agent reports still render as escaped literal text. Genuine usability improvement
remains a hypothesis to measure with new users, separate from engineering tests.

## Validation

Offline browser acceptance covers an empty project, actual fixture completion and
host checks, draft preservation and keyboard focus, failed/cancelled states,
missing reports and historical check results at desktop/390/320 widths. Existing
first-task and static-preview regressions exercise their real host boundaries;
synthetic display cases are explicitly fixtures, not additional AI generations.
