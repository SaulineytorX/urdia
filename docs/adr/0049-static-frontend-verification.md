# ADR 0049: Static frontend evidence through the existing check lifecycle

Status: accepted for private preview development.

Add frontend-static-v1 and frontend-counter-v1 to the host-owned verification
contracts. Preserve authenticated request/replay/consent/task-binding/busy/stale/
cancellation rules; models have no authorization or evidence authority.

Use a bundled Python helper with optional trusted Playwright runtime and Chromium
sandbox required. Execute an immutable bounded static snapshot in a fresh browser
context, served only on token-authenticated loopback. Restrict resource routing/CSP
and block service workers/downloads. Do not use --no-sandbox, install dependencies
from a web request or start arbitrary project build/dev-server commands.

Schema4 adds aggregate browser observations and fixed failure categories. Desktop/
mobile statuses, exact group counts, immutable source fingerprint and global status
are validated by host code before admission as evidence. Existing Python reports
retain their versions and cannot carry frontend fields/failure text.

Scope is static smoke/one specific counter contract, not a general frontend test
runner, subjective visual evaluator, production sandbox or proof of security.
Missing browser/runtime/sandbox fails closed. Initial install is an explicit
owner CLI operation; existing runtime is never overwritten. Native Windows remains
v2, and Windows-vault acceptance is unrelated to this Linux/WSL capability.
