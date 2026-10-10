# ADR 0036: Explicit profile advice bound to reviewed plan steps

- Status: Accepted (experimental preview)
- Date: 2026-10-04 UTC
- Extends: [ADR 0035](0035-host-session-project-plans.md)

## Context

Reviewed host-session plans and one-off profile recommendations exist separately.
The owner wants different suitable providers/models/effort for each project stage.
Unlinked advice could execute the right text while leaving the reviewed stage
unassociated, or execute an outdated stage after another tab changed the plan.

## Decision

Add an optional strict plan link (plan UUID, revision, stage index) to advisory
requests. Store it with the original objective and exact request replay cache;
expose only the link in authenticated job snapshots, and the original prompt/link
in the explicit execution preview. Existing engines, candidate coverage, English
routing questions, fallback policy and TypeSafe HTTP adapter remain unchanged.

Under the advisory lock, compare accepted requests before validating new linked
requests against ConversationHub's exact canonical prompt and current stage.
Validate again at preview and atomically at confirmed task admission using the
existing conversation lock. Bind only after successful command enqueue. Maintain
lock order advisory → conversation; no inverse call. An in-flight recommendation
can become stale; it cannot authorize stale execution. Exact accepted confirmation
replay precedes freshness checks and starts no second task. Ordinary advice retains
its existing independent behavior. Browser-supplied candidate/profile overrides
are not introduced; target identity remains host-retained and revalidated.

A stage button prepares an advice draft without requesting inference or executing.
Preserve unrelated drafts. The operator picks the engine, consents to remote advice
when applicable, requests advice, reviews the target/exact selectors and confirms
one run. Editing before a new advice request removes its stage link; editing after
admission cannot change the original execution preview. Subsequent ordinary tasks
keep the default profile. Result review is still separate from task completion.

Cache observed profile labels with stage task observations. Do not label a queued
recommended task with the default agent before its actual launch is observed.
Remote inference and suitability are not independently verified by those labels.

## Consequences and verification

A stage may use any host-configured executable profile supported by the engine.
This does not install agents/models, configure keys automatically, generate plans,
measure cost/quality, execute in parallel, advance stages automatically, expand
permissions or bypass ADR 0015. Typed recommendations and confidence are not proof
of correctness. No new semantic thresholds or API contracts are adopted.

Host tests cover exact links, request conflicts/replays, absent host state, stale
preview/confirm after clear, profile observations and manual/advisory admission
races. Offline browser acceptance uses a loopback Laya fixture with a different
profile/model/effort launch, explicit tool rejection, lost-confirm response replay,
refresh recovery and stale-stage refusal. It proves integration behavior, not
real engine routing quality; representative live quality evaluation remains separate.

Current TypeSafe documentation was checked on 2026-10-04: [API](https://docs.typesafe.ai/api),
[Choice](https://docs.typesafe.ai/primitives/choice), and
[function calling cookbook](https://docs.typesafe.ai/cookbooks/function_calling).
Keep known validation and execution in code and semantic selection advisory.
