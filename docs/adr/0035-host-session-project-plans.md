# ADR 0035: Reviewed plans owned by the host session

- Status: Accepted (experimental preview)
- Date: 2026-10-04 UTC
- Extends: [ADR 0034](0034-operator-project-plans.md)

## Context

The owner completed a real two-step Qwen plan in WSL. Tab-only state made refresh
lose the reviewed outline and could not enforce stage associations across tabs.
Keep operator authorship and explicit task/result review, while moving authoritative
bookkeeping into the same host boundary that already serializes tasks.

## Decision

Store one immutable reviewed outline in ConversationHub session memory, with a
host-generated plan UUID, monotonic revision, bounded fields and at most 32 accepted
mutation IDs. Authenticated create/clear/review requests use compare-and-swap
revision checks; exact payload/ID replays return current state without repeating
the operation. Reject malformed payloads with fixed error codes, without source
values. Mutations are serialized with active tasks and verification.

An optional prompt plan link includes plan UUID, revision and stage index. Validate
exact canonical text, previous reviews and retry eligibility under the admission
lock. Bind only after successful enqueue; failures leave the plan unchanged. An
accepted prompt's replay cache includes its original link, so retries remain valid
after revision changes without redispatch. Completed/failed/cancelled observations
are cached with the stage independently of bounded visible conversation history.
Review requires explicit confirmation and the exact completed bound task UUID.

Authenticated snapshots supply reviewed state to every tab; reload is inert.
Unreviewed form/composer/pending requests are still tab-local. No localStorage or
automatic disk plan persistence; restarting the host clears state.
[ADR 0037](0037-portable-reviewed-plan-recovery.md) adds explicit portable-file
export and reviewed recovery with a fresh identity, separate from live authority. Existing sent-prompt
retention and agent record boundaries remain. Show the objective as literal text.

## Consequences

Shared tabs cannot admit duplicate linked tasks from a stale revision. Host state
survives refresh but is not a durable project ledger. Definitions and mutation
caches contain private project text in process memory. Outline content is capped
at 64 KiB UTF-8 in addition to per-field and HTTP body bounds.

Scope is an instruction, not a file allowlist; review is not independent correctness.
Ordinary edited sends remain unlinked. [ADR 0036](0036-step-bound-profile-advice.md)
extends the boundary with explicit stage-bound advisory review; ordinary advice
remains unlinked. No automatic
chain, profile selection, permissions, trusted-session activation, model-generated
plans or parallel file writes are introduced. ADR 0015's evaluation gate is unchanged.

Tests must cover auth/redaction, mutation replay/conflict/bounds, admission rollback,
review gating, clear/send and two-tab races, history eviction, active/completed
reload, lost mutation response retry and existing browser/permission flows.
