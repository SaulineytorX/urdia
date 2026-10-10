# ADR 0037: Portable private plan files and reviewed recovery

- Status: Accepted (experimental preview)
- Date: 2026-10-04 UTC
- Extends: [ADR 0035](0035-host-session-project-plans.md), [ADR 0036](0036-step-bound-profile-advice.md)

## Context

Reviewed host-session plans survive browser refresh but disappear at host restart.
The owner wants to continue a project without losing its outline and prior notes.
Default content retention must remain unchanged; historical data cannot authorize
current execution, inherit tool permissions or pretend current verification.

## Decision

Provide explicit authenticated portable JSON export and inert import preview.
The browser downloads a fixed-name file only after a click; the host never chooses
an arbitrary filesystem destination or writes a plan automatically. Schema version
1 is strict, capped at 96 KiB with existing per-field/64 KiB outline limits and
bounded historical metadata. Export source project display name, wall time,
objective/definitions and latest task ID/status/agent/elapsed/review notes; omit
response text, profiles/selectors, credentials, permission receipts and tickets.
Default filename is ignored by the Urdia checkout. Auth and no-store apply.

Treat imported notes as file claims. Preview neither mutates host plan state nor
starts work. The operator reviews current files/project compatibility, picks an
original starting stage (first by default), confirms any earlier-stage omission
and explicitly restores. Changing the starting stage renews confirmation. Recovery
is a normal revision-bound, exact-replayable plan mutation serialized with tasks
and verification, requiring no active plan. It creates a fresh UUID and retains
only the remaining stages as live definitions, all unreviewed, attempt zero, task
None. No archived UUID, status, review flag, profile, permission or ticket becomes
current authority. Current stage admission/advice checks remain unchanged.

Retain the original archive/start point as separate session recovery context,
with historical notes clearly labelled. A continuation export preserves original
stage order and copied notes for omitted/not-yet-rerun stages, marked inherited;
current task observations replace their stage's latest note. No recursive archive
embedding or unbounded attempt ledger. Each file has at most eight original stages.
Composer/advice drafts remain separate. No current file correctness is inferred
from a historical Completed status or an imported review flag.

## Consequences and validation

This enables manual save/recovery across hosts and restarts without introducing a
server storage service, automatic restore or broader file access. The file is
private project content; arbitrary outline text is included. Workspace display
names are review context, not canonical-path identity or compatibility proof.
There is no autosave; an unsaved current host still loses plan state at restart.
Malformed/unsupported/oversized data is refused with fixed diagnostics. Historical
profile/model selectors are omitted; executable choices require current host config.

Test strict/authenticated export/preview, redacted malformed errors, size/bounds,
confirmation/start-index gating, fresh UUID/review/task state, exact restore replay,
old-link refusal, inherited continuation notes and real browser downloads followed
by recovery in a distinct disposable host. Verify literal text and mobile layout.
No live AI calls or routing-quality claims; ADR 0015 remains unchanged.
