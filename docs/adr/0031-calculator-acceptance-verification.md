# ADR 0031: Explicit calculator acceptance verification

- Status: accepted (source preview)
- Date: 2026-10-03

## Decision

Introduce an explicit operator-run Linux/WSL helper for calculator-add-v1 before
exposing general-purpose verification in the browser. Require a fixed workspace,
regular bounded acceptance files and --confirm-execution. Host code owns the
three add examples and runs them independently of the project's unittest suite
in separate isolated Python processes. Require a nonempty suite without skips.
Both checks must exit successfully and before/after acceptance file hashes must
match. Use the existing smoke supervisor for deadlines and descendant cleanup.

Discard all project stdout/stderr; return only fixed names, hashes, process
outcomes and monotonic durations. Omit inherited environment credentials and use
a temporary HOME. Do not introduce AI calls, arbitrary command input, durable
content retention, model-granted permissions or automatic routing.

## Boundaries

This helper runs project code as the current user. It is not a sandbox or
malicious-code attestation, and hashes cover only two files at two instants.
It does not bind evidence to a browser turn, verify all project requirements,
attest remote inference or enter the minimal binary package. No dashboard badge
may imply this check ran before a future authenticated integration explicitly
records its result. See [verification guide](../verification.md).

The operator flag authorizes this separate local command; it does not reuse
or change ACP permission receipts or a trusted session. A future host/browser
check runner needs its own authorization and stale-result lifecycle design.
