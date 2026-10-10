# ADR 0022: Optional native Codex app-server bridge

- Date: 2026-10-02
- Status: Accepted for experimental explicit-provider use

## Context

The second real agent must reuse the owner's installed authenticated Codex, keep
agent execution separate from semantic routing, and preserve host authorization.
The evaluated external ACP adapter embedded an older runtime and rejected the
model that the installed native CLI successfully executed with the same account.

## Decision

Add a stdlib Python source bridge from ACP v1 to Codex 0.159.2 app-server over
stdio. Pin the supported runtime, confirm read-only/on-request/user settings,
forward text and redacted tool reports, and translate only scoped command/file
approvals to one-shot ACP requests. Never select persistent approvals or grant
root/network/policy expansion. Unknown callbacks fail closed. Bound resource use,
reject stale/cancelled approvals, interrupt turns and reap the native process group.

Native Codex owns authentication; no key copying, installation, CLI update or
browser credential storage. Registration uses existing external provider config.
Launch controls are confirmed once; live changes remain unsupported. Thread
identifiers and turn identifiers are checked before forwarding native activity.

## Consequences

This is an optional Linux/WSL experimental adapter, not a bundled CLI or stable
cross-version promise. Doctor's executable discovery cannot prove inference
entitlements. Native tool reports and model settings remain provider claims.
Ephemeral threads do not guarantee no native/provider retention. The bridge is
not a sandbox and explicit approval may allow broader native execution.

Offline regressions run in workspace tests; live account checks are explicit,
bounded and excluded from CI. Automatic routing stays disabled under ADR 0015;
no frozen evaluation artifacts or selection policy change in this slice.

## 2026-10-05 clarification: authoritative final text

Native `item/completed` agent-message text is authoritative under the pinned
App Server contract. Track bounded streamed prefixes per scoped item and append
only a native-supplied missing suffix before turn completion. Exact repeated
finals are idempotent; disagreement, late deltas, missing final state on success,
and unsupported interleaving fail closed because ACP text is append-only. Do not
rewrite output, repair arbitrary JSON or retry inference automatically. Message
state is cleared between prompts. Offline protocol and real ACP SDK burst tests
cover this contract without using an account.
