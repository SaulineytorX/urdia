# ADR 0033: Reviewed calculator contracts

- Status: Accepted (experimental preview)
- Date: 2026-10-03
- Extends: [ADR 0032](0032-dashboard-calculator-verification.md)

## Context

The owner wants an actual five-number calculator task, with evidence independent
of agent claims. Three examples of `add(a, b)` do not verify that behavior.

## Decision

Add the host-owned `calculator-five-v1` Python contract alongside the unchanged
`calculator-add-v1` default. The full API, errors and twelve representative test
groups are documented in [verification](../verification.md). The CLI helper
accepts an allowlisted `--contract`; the authenticated dashboard request accepts
a typed enum. Neither accepts code, commands, paths, interpreters or deadlines.

Bind the selected contract to the immutable accepted job and host command. A
matching replay reuses the job; changing the contract with the same request ID
conflicts. The bundled runner and hub both require report-contract agreement.
Reports retain the two-file/two-check evidence shape. The subsequent counted
report extension emits schema version 2; legacy version-1 observations remain
readable without inferring counts. See the extension below.
Requests without a contract remain backward compatible with addition.

The dashboard shows the specification before confirmation, resets consent when
selection changes and locks selection during active or uncertain work. A task
preparation button only fills an empty composer; execution still requires sending
the reviewed draft. Evidence labels use the accepted job, not the current selector.

## Consequences

No additional inference, automatic dispatch, permission grants or public listeners.
The fixed supervisor, time limits, cancellation, redaction, caps and hash checks
remain unchanged. Existing agent adapters need no changes. The feature verifies
a Python calculation API; graphical calculator acceptance and arbitrary task
verification are separate. Same-user execution is not a security sandbox, and
representative tests are not an exhaustive mathematical or malicious-code proof.

## Aggregate test observations (2026-10-03 UTC)

The checker writes a private, bounded aggregate summary after each unittest run,
separate from discarded project stdout/stderr. Emit six allowlisted nonnegative
integer counters (maximum one million each), with no names or exception text.
Version-2 passing checks require nonempty successful counts and exact host test
group count for the selected contract. A missing summary with exit zero is refused.
Expected failures keep unittest semantics; skips and unexpected successes fail.
Subtest failures are not constrained to be at most methods run. The dashboard
labels absent counts explicitly and never derives them from agent prose.

The bundled host requires version 2; the report reader retains legacy version-1
compatibility for reports with no counts. These same-user observations remain
subject to the existing malicious-workspace limitations; no sandbox is introduced.
