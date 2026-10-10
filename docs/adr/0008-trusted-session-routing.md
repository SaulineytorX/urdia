# ADR 0008: Trusted sessions and semantic routing

- Status: accepted
- Date: 2026-09-27
- Supersedes: none

## Context

Urdia should eventually support long-running sessions where an operator does
not have to review every routine operation. It should also be able to route a
new task among prepared providers or workflows using Jev, Laya, TypeLLM, a
local model, or deterministic rules.

The original reference for this direction was
[`codejunkie99/keel`](https://github.com/codejunkie99/keel). Its useful
boundary is that the host prepares eligible routes, a decision engine selects
an opaque identifier, and the host validates the selection before applying it.
The decision does not grant a tool permission.

Urdia already separates `DecisionProvider` from `PermissionBroker`. Any
trusted mode must preserve that separation and the existing receipt path.

## Decision

Add an explicitly activated, time-limited trusted-session policy. It is
bound to one Urdia session, canonical workspace, provider session, and an
allowlist of capabilities. It will expire automatically, be revocable from
the local UI, and create a redacted receipt for every operation it authorizes.
It is never a global or implicit default.

The UI uses “Trusted session” rather than
“allow everything” because the latter hides the actual boundary. Activation
requires a separate confirmation that shows the session, workspace,
provider, capabilities, and expiry.

Decision providers may later recommend one of these host-owned routes:

- evaluate against the active trusted-session policy;
- request one-shot interactive review;
- deny or use a deterministic fallback; or
- escalate because the prepared choices are insufficient.

For Jev, prompts and criteria will be written in English. Urdia will send only
prepared opaque candidate IDs plus the minimum task context required for the
judgment. It will record the exact model version, candidate-set fingerprint,
probability semantics, confidence, selected ID, and fallback reason. The host
will reject invented, stale, unavailable, or out-of-scope IDs.

Semantic output is advisory. A recommendation to use the trusted-session path
has no effect unless the operator already activated a matching policy and the
request passes deterministic session, workspace, provider, capability,
revision, expiry, and revocation checks. Missing credentials, timeouts,
invalid responses, low confidence, and model errors fail to an explicit
fallback; they never create an authorization.

## Consequences

- Routine operations can proceed without one browser decision per request
  while remaining inside an operator-defined session boundary.
- Jev can route work in the style that inspired Urdia without becoming a
  security authority or receiving executable commands as candidates.
- Trusted sessions need their own lifecycle, revocation, expiry, receipts,
  and evaluation suite before release.
- Confidence thresholds cannot be copied from another project. They must be
  calibrated on representative Urdia tasks against a pinned model version.
- This proposal does not change the current deny-by-default and one-shot
  permission behavior.

## Acceptance criteria

- Test policy activation, expiry, revocation, scope mismatch, and restart.
- Verify every trusted operation produces a receipt before authorization.
- Verify Jev cannot activate, widen, extend, or persist a trusted policy.
- Evaluate routing accuracy and fallback behavior on representative tasks.
- Verify model/network failures and unknown candidate IDs fail closed.
- Review the browser wording and audit projection before exposing the mode.

## Implemented validation

- `--trusted-session` requires the authenticated permission UI and remains off
  by default. It makes activation available but does not activate it.
- Activation is derived from a real pending request and uses separate prepare
  and confirm calls. The challenge is one-use and expires after two minutes.
- The policy lasts 15 minutes and enumerates every current operation kind. It
  matches the Urdia session, negotiated provider session, canonical workspace,
  capability, and a current-or-later prompt revision.
- Expiry, revocation, scope mismatch, duplicate/stale confirmation, and a new
  process with no restored policy are covered by deterministic tests.
- ACP persists a schema-v2 redacted trusted receipt before release, selects
  only its one-shot option, and revalidates policy identity, generation, scope,
  expiry, and revocation under the prompt response gate.
- Trusted provenance cannot be deserialized from browser JSON. Decision
  providers remain behind the separate advisory interface and cannot create,
  widen, renew, persist, revoke, or apply a trusted policy.
- The browser shows provider, canonical workspace, complete capability list,
  lifetime, and expiry. It never receives provider-session IDs, raw arguments,
  commands, locations, operation fingerprints, or policy credentials.

The trusted allowlist includes destructive and process capabilities. This is a
deliberate local operator mode, not a sandbox. ACP children still run as the
same operating-system user as Urdia.
