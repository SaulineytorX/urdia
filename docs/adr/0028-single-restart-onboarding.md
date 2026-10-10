# ADR 0028: Prepare agent and profile with one final restart

Status: Accepted (experimental)
Date: 2026-10-02

## Context

Onboarding previously required restarting once to expose a newly registered agent
in the profile editor, then again to apply its saved profile. Registration already
persists an explicitly trusted launch and inspection already requires independent
confirmed temporary startup. A setup-only staging view can remove the first restart
without mutating active execution configuration.

## Decision

After agent-store persistence succeeds, append its exact host-owned launch to the
profile editor's shadow registry and the explicit inspection service. Do not
modify the active registry, advice candidates, execution maps, primary session,
review tickets or task settings. The additions are append-only; existing launches
cannot be replaced. A failed or replayed registration cannot stage another entry.

The editor exposes a pending-restart flag, increases its revision, preserves
unsaved drafts and accepts profiles referring to these persisted launches. The
same catalog validation, private file checks and concurrency locks still apply.
No executable/argv is exposed in metadata. Profile saves do not start agents.
Inspection of a pending launch requires the same separate startup confirmation,
fixed workspace, deny-by-default permissions, deadline and cached-request limits.
It remains a prompt-free temporary session, not task activation.

Register the agent, inspect its selectors, save its profile/default and restart
once with both stores. Startup loads launches before validating profiles and
activates the saved setup together. Missing launch stores or flags fail profile
validation instead of selecting an unintended provider. Pending entries remain
excluded from current recommendations and task execution until that restart.

## Consequences

This supersedes the intermediate-restart requirement in ADRs 0026/0027 and the
setup-only provider restriction in ADR 0025. It does not enable live execution
catalog replacement or switch the primary conversation without restart.
Installed/login prerequisites remain unchanged; third-party startup can have
native effects. Urdia is not a sandbox. No secret copying, inference prompt,
automatic routing or permissions/trust change is introduced. ADR 0015 and frozen
routing evaluation artifacts remain unchanged.
