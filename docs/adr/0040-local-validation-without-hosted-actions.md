# ADR 0040: Local validation without hosted Actions

Date: 2026-10-05
Status: Accepted by explicit owner request

## Context

The owner requested removal of GitHub CI to avoid possible runner costs. The
repository had automatic push/PR CI and manual candidate preparation. Both can
allocate hosted runners, so retaining the manual workflow would leave that path.

## Decision

Move both unchanged definitions to `.github/disabled-workflows`, outside GitHub's
workflow discovery directory. Current main has no active workflow files. Preserve
local formatting, Clippy, workspace tests, JavaScript syntax, dependency policy,
smoke and candidate checks. Release evidence must refer to the exact locally
validated commit; absence of hosted CI is not an exemption from release gates.

Do not restore active files or dispatch historical workflows without a new
explicit owner request. Use `[skip ci]` on the removal commit as an additional
push/PR skip instruction. Keep the repository private and publish no artifacts.

## Limits

Git removal cannot cancel jobs already queued/running, change global Actions
permissions, or verify charges. This session has SSH push access but no available
Actions cancellation/settings capability. Only main was advertised by the remote
at review time. Historical refs may still contain workflow definitions; a
repository-wide disable in GitHub settings is the additional hard stop.

Agent reconnection remains the next bounded product slice. No agent behavior,
credentials, installed services, listeners or permissions change here.
