# ADR 0017: Explicit browser advisory routing

- Status: accepted
- Date: 2026-10-02
- Extends: ADR 0016's visual workspace; does not supersede ADR 0015.

## Decision

`urdia chat --advisory-ui` opts into authenticated browser recommendations.
Without the flag, `/api/advisory` returns 404 after authentication. The historical
UI and ordinary conversation bindings have no advisory controller.

The CLI prepares available registry candidates and decision adapters at startup.
Configured means the adapter could load its host configuration, not that its
credential, connection or model quality has been verified. The browser cannot
provide keys, endpoints, candidates, confidence policies or execution commands.
API keys remain in the existing host adapter. This does not change the MVP's
same-user/inherited-agent-environment trust boundary.

GET returns connection metadata and the latest in-memory result. POST accepts
only a UUID, engine ID, bounded objective and explicit `allow_remote` consent.
The host independently requires consent for Jev. Authentication precedes JSON
parsing; malformed values are not echoed. There is one concurrent request,
a 35-second host deadline and at most 64 accepted keys per process. Matching
keys replay accepted jobs without another adapter invocation; changed payloads
conflict. Jobs survive HTTP disconnection but not host restart. Existing adapters
may retry upstream HTTP calls under their own bounded policies.

The callback passes results through the existing RouteResolver and maps opaque
candidate IDs back to registered agents. Provider errors, no selection, invalid
decisions and confidence fallback remain distinct. Objectives and results are
not persisted by this feature. The browser uses DOM text and distinguishes
reported probability semantics from task success.

No result switches or starts an agent, submits a conversation prompt, grants a
permission or activates trust. Automatic routing remains disabled. Persistent
profiles, browser key entry, credential removal and manual agent switching need
separate designs. New routing profiles still require the frozen evaluation gate.

## Evidence

Offline host tests cover authentication, input redaction, idempotency, contention,
explicit remote consent, limits and deadlines. Browser acceptance exercises a
real deterministic host recommendation without agent execution, and controlled
remote consent/review/error fixtures. No live model call is part of validation.

## Host decision observations — 2026-10-02

Each admitted advisory record keeps a private monotonic admission instant and
exposes optional elapsed_ms only at its terminal outcome, including errors and
timeouts. Replay preserves the frozen value; polling makes no inference calls.
The bounded authenticated in-memory job owns this observation, not SQLite. It
includes adapter retries and scheduling, not just model computation. The UI
distinguishes this latency from agent execution and explicitly states that the
recommendation has not independently verified the requested task. No token/cost
estimate, automatic fallback cascade or authorization policy is introduced.
