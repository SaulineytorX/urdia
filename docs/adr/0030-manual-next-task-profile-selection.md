# ADR 0030: Manual next-task profile selection

Status: Accepted (experimental)
Date: 2026-10-02

## Decision

Allow authenticated manual selection of an already loaded execution profile for
future ordinary browser tasks without restarting the host. This is an in-memory
next-task default, not a live replacement of the primary ACP session or persisted
profile catalog. Host startup builds immutable choices from active profiles with
available registered launches, exact model selectors and effort selectors when
an effort label is declared. Pending editor registrations/profiles are excluded.
The feature does not depend on enabling advisory provider APIs.

POST /api/conversation/profile requires the observed selection revision, a
host-known profile ID (null restores the original session) and explicit confirm.
It starts no agent or decision call. Under the same conversation mutex as task
admission, selection rejects an active queued/running/cancelling turn and stale
revision. Its revision is independent of live-control/telemetry updates.
Selections advance that revision, including a repeated no-op choice. A lost
selection response requires reading current state and reviewing it; the server
does not automatically replay or retarget a stale request.

Ordinary POST /api/conversation/prompt includes profile_revision whenever the host
exposes choices. The host resolves its target under that same lock and binds it
to the accepted turn. Requests missing/stale revisions cannot run. Retained
idempotency keys bind text, source (ordinary/advisory) and selection revision;
an accepted retry after later selection changes returns the original turn with
no additional command. A new revision with the same turn ID conflicts.

Selected-profile tasks use ADR 0024's fresh-session executor: fixed workspace,
45-second cancellable startup, exact startup/live-selector agreement before any
prompt, no fallback, one-shot broker or deny by default, durable redacted receipts
and separate session lifecycle events. Earlier transcript text is never forwarded.
Each task closes its fresh session; the primary session stays alive for explicit
restoration. Original-session tasks keep their original conversation context.
Agent reports are not independent proof of remote inference or code correctness.

Trusted-session hosts are excluded entirely, even before activation. Their
provider/workspace permission scope is not migrated. Advice review tickets retain
their immutable one-off targets and can explicitly override this manual default
for one task; they do not change it or grant permissions. The UI distinguishes
applied choice from an unsaved preview and from an agent actually connected.

## Consequences

This extends ADRs 0024/0028 for already loaded profiles only. Saving new launches,
changing profile definitions or the persisted default still requires a restart.
Choices disappear on host restart. No automatic routing, decision calls, new
listeners, credentials or permission capabilities are introduced. ADR 0015 and
frozen evaluation artifacts stay unchanged. Multiple tasks remain sequential;
planning, parallel agents and live trusted-scope switching remain separate work.

## 2026-10-05 handoff clarification

Reviewed project steps pass only their canonical objective/current scope/acceptance
into fresh selected-profile tasks. Earlier conversation and other stage replies
are not forwarded. Make answered requirements explicit in those reviewed fields;
this is deliberate provider/privacy separation, not automatic conversation-memory
migration. The UI now explains this before acceptance and selection. Offline
handoff tests check omitted prior-step sentinel, exact context, confirmed selectors,
fresh session lifecycle and stage UUID. A live Codex low-to-medium handoff passed
independent repair checks; Qwen-to-Codex remains unverified due an observed native
authentication error. No permission, routing or transcript-retention policy changed.
