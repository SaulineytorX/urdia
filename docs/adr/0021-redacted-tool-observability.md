# ADR 0021: Redacted tool observability

- Status: accepted
- Date: 2026-10-02
- Extends: [ADR 0007](0007-authenticated-streaming-conversation.md)

## Context

Operators need to distinguish agent text from reported file/command activity.
ACP tool payloads can contain credentials, paths, code and terminal output.
Displaying those payloads would undermine the existing redacted event boundary.

## Decision

Extend the provider-neutral update sink with fixed typed tool category/status,
a turn-local ordinal and a truncation signal. The ACP projection retains bounded
raw tool IDs privately for correlation, excludes all other payload fields and
accepts only current-session notifications while a prompt sink is active.
The conversation hub independently rejects tool activity outside a running turn.

Retain at most 128 tools per turn, with IDs limited to 1024 printable ASCII bytes
before copying. Partial reports keep unreported status unknown, duplicate reports
are ignored, status regressions are suppressed and the first terminal report is
frozen. Retained tools can still update when the cap is reached. Expose only last
states in authenticated memory snapshots, pruned with the 64-turn history. Show
the latest eight entries and retained-set counts, including a truncation warning.

Notifications are observations supplied by the agent. They neither verify side
effects nor grant permission, select a model, activate trust, or create durable
execution receipts. Closing/cancelling a turn does not invent outcomes for tools
that never reported completion. Existing permission receipts remain separate.
No tool titles, original IDs, names, arguments, paths, results or metadata reach
the new telemetry API, debug representation or SQLite storage.

## Consequences

Agents without tool reports keep working, with no fabricated activity. A reported
failed tool can appear in an otherwise completed conversation turn; these are
different states. ACP notifications have no prompt-turn identifier, so delayed
same-session messages arriving during a later active turn cannot always be
attributed to the original turn. The UI must not present these observations as
proof of execution, authorization or independently verified task success.

The cap and loss of detailed payloads trade full debugging for bounded private
observability. Exact token/cost totals and automatic routing remain unchanged.
See [workspace UI](../workspace-ui.md#redacted-tool-activity) and the
[ACP v1 contract](https://agentclientprotocol.com/protocol/v1/tool-calls).
