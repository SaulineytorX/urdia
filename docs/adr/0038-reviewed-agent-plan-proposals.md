# ADR 0038: Agent proposals edited before plan acceptance

- Status: Accepted (experimental, operator-reviewed)
- Date: 2026-10-04 UTC

## Decision

Add an objective-first proposal flow to the existing authenticated conversation.
Ask my agent for a plan prepares an English, tool-free planning instruction in the
composer, preserves unrelated drafts, and starts no model request. The operator
reviews the selected agent and explicitly sends using the ordinary task admission,
cancellation, permission and profile controls. Quota use belongs to that agent.
Planning instructions do not enforce a sandbox or expand permission authority.

Review latest response as a plan submits the latest completed response to an
authenticated no-store parser. Accept only one JSON outline or a single whole
JSON/plain fenced block. Strict unknown-field rejection and existing outline bounds
apply, plus a 96 KiB input cap. Validation is inert; no host plan, revision, task,
profile or permission changes. Fixed errors never repeat private source text.
This endpoint accepts untrusted text, not a proof of agent provenance or quality.

Display validated text literally in a separate proposal preview. Explicit Use this
proposal replaces only the editable browser outline, preserving the chat composer.
Require the source turn and plan revision to remain current. Applying a proposal
creates no host plan: the operator adjusts fields and explicitly selects Review
plan via existing revision-bound mutation. Browser refresh discards unaccepted
proposal/editor drafts; accepted plans retain existing host-session/export behavior.

Jev/Laya remain profile recommendation engines; no question/threshold/wire change.
Jev generates typed decisions, not free-form plans; current TypeSafe index and
coding-agent/System One documentation were read for this design. Planning quality
requires the representative rubric in ../planning-evaluation.md; offline fixtures
prove interaction boundaries only. Automatic dispatch remains blocked by ADR 0015.

## Verification

Rust tests cover single JSON/fence parsing, schema/field/count/size rejection,
authentication, no-store, private fixed errors and zero state/command mutation.
Offline browser acceptance exercises inert preparation, preserved composer and
outline on failure, a literal response fixture, explicit editor replacement and
acceptance without a second task, desktop/mobile layouts. No real model planning
quality is inferred from the fixture.

## 2026-10-05 navigation clarification

Expose an explicit return action for the latest completed planning-prefixed turn
while no plan is accepted. It opens the existing panel and invokes the existing
preview handler, with the same busy/freshness checks and no new endpoint or
permission. Prefix recognition is only presentation: strict host parsing remains
authoritative. No automatic parsing, adoption, task dispatch or draft replacement.
