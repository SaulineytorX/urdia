# ADR 0042: Observed project summary

- Status: accepted
- Date: 2026-10-06 UTC

## Decision

Derive a read-only ProjectSummary in the authenticated conversation snapshot from
host plan tasks, operator review flags, measured clocks and retained verification
jobs. Never derive success from agent prose, model labels, elapsed time or archived
history. Keep completed, reviewed and check-passed counts separate. No AI call,
admission, review, export, permission or routing mutation is introduced.

Link each row to its latest bound task UUID. Preserve a bounded chronological
verification admission order (maximum16 existing jobs), distinct from random UUID
ordering. Use the latest request for that exact task, not an older passing request
or another attempt. Validate report/contract before projecting counts/durations;
missing evidence never counts as passed. Later host tasks mark checks historical,
not proof of current file identity. Imported outline history remains separate.

Sum measured latest-attempt durations only, using live clocks when retained and
stored plan observations after transcript eviction. Expose the timed-step count
and missing observations. Verification time and prior retries are separate;
tokens/cost stay unreported. Test counts describe executions across the two checks,
not unique tests. The summary contains no raw logs, hashes, arguments or responses.

Render labels and private user titles with textContent, reuse row elements, and
link only to retained task transcripts. Idle polling preserves focus. Public shell
contains no project summary data; existing token auth protects its projection.

## Validation and limits

Deterministic tests cover independent completion/review/check counts, live versus
retained duration, missing measurements/test counts, exact-attempt binding,
chronological newest cancellation, rejected invalid passing evidence and omitted
private fields. Offline browser acceptance uses actual bundled checks and two
explicit linked tasks, with separate review, historical check labeling, literal
HTML titles, task navigation, no additional tasks and mobile layout.

This is memory-only evidence of recorded observations, not arbitrary-project
verification, a changed-file audit, a durable completion report or release approval.
Existing fixed calculator contracts and explicit execution consent are unchanged.
ADR0015's automatic-routing gate and ADR0040's inactive GitHub workflows remain.
