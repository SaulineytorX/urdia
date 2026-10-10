# ADR 0048: Guided source setup without elevated installation

Status: accepted for private Linux/WSL preview development.

## Decision

Offer scripts/setup-urdia.py as the first source-setup assistant. It checks
prerequisites without installing them, asks before a pinned locked build, chooses
an offline demo or discovered supported agent, selects/creates an explicitly
approved workspace, and asks again before launching the existing authenticated
loopback project launcher. It does not collect account secrets or submit a task.

Reuse host permission/advisory boundaries; no alternate web server or auth layer.
Real-agent launch/profile stores are opt-in through this path and live outside the
project in the existing private state location. Codex registration contains only
argv/path metadata, is tied to its supported experimental version and never
silently overwrites edited settings. Matching private files are reusable; unsafe
modes, hardlinks and symlinks are refused. No shell interpolation.

## Scope

A terminal assistant, not a native Windows installer or all-agent installer.
No sudo/system installation, login, PATH change, global binary overwrite, production
signing, auto browser helper, listener beyond loopback or automatic routing.
The first source preview still needs installed prerequisites; later graphical
installation/upgrade delivery is separate. Agent availability is not authentication.

## Validation

Offline tests cover cancellation before settings/session effects, read-only check,
invalid workspace before build, private Codex argv with spaces, preservation of
edited/redirected settings, and a real authenticated streaming demo from a Git-free
layout with unauthenticated refusal, unchanged project and omitted token/text in
SQLite. An actual guided locked-build-to-demo rehearsal is recorded separately.
Existing source candidates predate this feature; refresh before delivery.
