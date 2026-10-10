# ADR 0032: Confirmed dashboard calculator verification

- Status: accepted (experimental)
- Date: 2026-10-03
- Extends: ADR 0031; updates its source-only integration boundary.

## Decision

`urdia chat --verification-ui` explicitly enables a fixed calculator-add-v1 check
on Linux/WSL with /usr/bin/python3 and /bin/kill. The binary embeds the first-party
checker and Linux supervisor; it needs no source checkout at runtime. The flag
is off by default and prerequisites are checked before agent startup.

Authenticated POST /api/conversation/verify accepts only request_id, turn_id and
confirm_execution. It cannot supply a workspace, command, interpreter, timeout,
contract, environment or model result. Require explicit confirmation and the
latest retained completed conversation turn. Job admission and ordinary/manual/
advisory task admission share the conversation mutex: exactly one task or check
can win. Switching profiles is blocked while verification is active. Fixed checks
run against the host's canonical workspace, independently of the coding agent.
This is an as-of workspace observation associated with a turn, not proof that
that turn created the checked bytes.

Prompt IDs and check request IDs cannot collide, preserving cancellation/join
identity across the host worker loop. Matching idempotency keys reuse the existing job without executing again, even
after a new task; changed turn bindings conflict. Keep at most 16 admitted jobs
per host, expose the latest through the authenticated conversation snapshot and
mark it historical when any later task is admitted. External edits cannot be
tracked continuously; show the file-hash and check-time scope explicitly.

## Execution and cleanup

Bundle trusted Python programs into a private temporary directory. An outer
child-subreaper supervisor has a 35-second deadline and one-second termination
period; the inner helper retains its 10-second per-check limits. Clear the child
environment and use a temporary HOME. Raw project stdout/stderr goes to /dev/null.
Read at most 8192 report bytes, validate the exact schema/contract/file/check keys,
SHA-256 forms and consistent exit/outcome fields before exposing any report.

Explicit POST /api/conversation/verify/cancel targets a known request_id. Queued
cancellation starts no process. Running cancellation signals the outer supervisor
and holds the busy slot until descendant cleanup completes. Host Ctrl+C also
signals and joins the worker before returning. Cancellation wins over a late
passing report when admitted first. Errors, refused files, unavailable Python
APIs and malformed reports never produce a passing badge.

## Boundaries

This is a separate operator authorization to run local Python, not an ACP tool
permission or model decision. No trusted-session authority is reused. Checks run
as the Urdia user and are not a sandbox or malicious-code attestation. Idle native
agent processes and external editors are not a global workspace lock. Only the
two acceptance files are fingerprinted; no general project success is inferred.

Jobs and reports are session-only; SQLite and prompt/response retention stay
unchanged. No AI call, token estimate, new listener or automatic routing is added.
ADR 0015 remains unchanged. General contracts, durable evidence and native Windows
support need separate designs. See [verification](../verification.md).
