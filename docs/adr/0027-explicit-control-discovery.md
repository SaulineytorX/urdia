# ADR 0027: Explicit bounded model/effort inspection

Status: Accepted (experimental)
Date: 2026-10-02

## Context

The dashboard profile editor previously required opaque ACP selector IDs to be
entered manually. Effort options can depend on the selected model; a static list
or display label does not establish executable settings or account entitlement.

## Decision

Expose only validated model/thought-level value IDs and current values through
a default-optional `AgentSession::control_options` contract. ACP reuses its
existing bounded selector parser; mode/permission controls, labels, descriptions
and private metadata are omitted. Malformed live updates clear the options.

An authenticated explicit inspection request starts an independent session for a
registered available launch in the host's fixed workspace. The user must confirm
startup. The request contains a UUID, provider ID and optional exact model ID;
no prompt, command, workspace, key or permission policy can be supplied. The host
starts it with deny-by-default permissions and the durable redacted receipt sink.
It sends no prompt and drops the temporary session after reading validated options.
Startup may contact the agent's service or create native records; Urdia cannot
promise a trusted third-party executable performs no work during initialization.
The operator must already trust its registered launch; Urdia is not a sandbox.

There is one inspection at a time, eight admitted request IDs per host and a
30-second startup/inspection deadline. Results/errors are cached in memory and
bound to provider/model. Same-key retries do not spawn again, differing scopes
conflict, and disconnected clients do not cancel the bounded cache-producing task.
Completion cleanup releases the busy gate on failure/panic. Error details are
redacted. Third-party adapter outputs are also validated: at most 128 unique
model/effort IDs each, bounded printable IDs, valid current membership and exact
requested-model confirmation. Unsupported/empty catalogs remain unavailable.

Choosing a reported model fills its draft pin/label and clears effort choices.
Another explicit inspection selects that model in a new temporary session and
returns its refreshed effort choices. Results are discarded if the draft scope
changed while waiting. Inputs are not overwritten during polling. Uncertain
HTTP delivery reuses its UUID for the same draft selection. Options are displayed
as inert text and are not persisted automatically. Saving still requires the
explicit profile store and affects the next launch only.

## Consequences

Users can discover reported selectors without typing them or sending a coding
task. Adapters without catalogs keep manual configuration; the native Codex bridge
currently reports confirmed current selectors, not its complete model catalog.
Reported choices do not prove entitlements, calibrated quality or actual remote
inference. Execution revalidates exact settings as before.

No installation, login, credential copying, automatic routing, trust, permission
or running-task change is introduced. Automatic orchestration, broad account
model discovery and multi-task concurrency remain separate work. ADR 0015 and
frozen routing evaluation artifacts remain unchanged.

Update: [ADR 0028](0028-single-restart-onboarding.md) allows setup-only staging
of persisted new launches so agent and profile configuration need one final
restart. Execution configuration remains immutable until that restart.
