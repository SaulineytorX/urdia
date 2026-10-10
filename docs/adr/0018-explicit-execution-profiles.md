# ADR 0018: Explicit execution profiles and defaults

- Status: accepted
- Date: 2026-10-02

## Decision

Host configuration may define up to 64 named execution profiles and an optional
default profile. Each references a dedicated registered ACP provider launch and
labels its coding agent, inference provider, model and optional effort.
`run` and `chat` select an explicit profile, an explicit provider, or the
configured default, in that precedence. Conflicting explicit flags are rejected.
There is no implicit default agent when neither a selection nor a default exists.

The launch uses existing provider commands and exact argv; label values never
become shell text, CLI flags or API parameters. Labels are operator declarations,
not verification evidence. Listings explicitly mark model/effort unverified and
separate executable discovery from model support. Labels are bounded, IDs unique,
references checked and ambiguous reuse of one launch by multiple profiles
rejected. Configuration errors do not echo profile source values.

This does not change decision adapters, candidate fingerprints, browser routing,
trust policy, stored event shapes or the ADR 0015 activation gate. Advice still
chooses registry provider IDs. No automatic task decomposition, concurrent
execution, profile switching, key handling or release publication is introduced.

## Consequences

Operators can launch a named configuration or use their configured default now,
without assuming the same model/effort options across unrelated agents. They
must independently verify the underlying ACP launch and model settings. Native
capability discovery, verifiable effective model/effort, profile-aware routing
and a task scheduler require subsequent contracts and evidence.

Multi-task execution will need explicit dependencies, bounded concurrency,
workspace/file ownership, cancellation propagation and recovery. A future
backend/frontend plan cannot permit conflicting writes to the same working tree
without host coordination. The user's default profile must remain available for
single-agent execution and as an explicit policy input; a decision model cannot
silently override a pinned user choice.

## Control evidence checkpoint — 2026-10-02

A disposable Qwen 0.24.7 ACP probe now verifies advertised selectors and reported
round-trip changes with no prompts/inference. Its model-dependent thought options
are documented in [Qwen controls](../qwen-controls.md). This does not update
profile verification fields, add native runtime control requests or prove remote
model settings. Host integration must keep declarations, agent-reported settings,
provider verification and task-quality evidence distinct, and refresh selectors
when a model changes. Never use a mode selector as an effort control.

## Native startup control checkpoint — 2026-10-02

Optional `profile.session_controls` now carries explicit opaque ACP model/effort
values; display labels retain their prior meaning. Startup discovers unambiguous
categorized select controls, applies model first, refreshes effort choices and
confirms requested values again after both changes. Unsupported/malformed or
unconfirmed requests abort before prompt. Mode and permission options cannot be
selected through this interface. Legacy no-control launches remain supported.
Direct provider selection bypasses the profile's intent. Changes remain startup-only.

Authenticated context exposes a redacted initial agent report independently of
labels. It is memory-only, not remote inference verification or live state; later
agent notifications are not tracked here. Existing verification flags remain false.
Actual Qwen 0.24.7 native-host checks confirmed two fixture configurations and
rejected stale effort with zero prompts/endpoint requests. No real quota was used.
Automatic execution still requires the ADR 0015 evaluation gate and configuration
binding; profile advice and all permission authority remain unchanged.
