# ADR 0019: Experimental advice over declared execution profiles

- Status: accepted
- Date: 2026-10-02
- Extends ADR 0017 and ADR 0018; does not supersede ADR 0015.

## Decision

`chat --advisory-ui --advisory-profiles` explicitly changes browser advice from
bare provider candidates to configured execution profiles. The source launcher
forwards the flags. Ordinary `--advisory-ui` retains its previous provider mode;
CLI `route`, pinned adapter prompts/models and frozen evaluation corpora remain
unchanged. There is no implicit mode change just because profiles exist.

The host freezes available profile candidates at startup and exposes case-local
opaque route IDs to its existing resolver/adapters. It excludes profiles whose
registered launch executable is unavailable and rejects an empty or oversized
profile state before database creation, agent startup or listener creation.
The state schema is `urdia-declared-profile-advice-v1`, bounded to the existing
128 KiB limit. Candidate descriptions are English and refer to corresponding
entries in the nested execution_profiles context map.

State contains validated profile labels, executable discovery, conservative
authentication/inference metadata, the configured default and explicit unknown
quality/latency/cost fields. Model/effort verification is always false. No launch
commands, argv, credential values, project contents or parent workspace paths
are included. Labels are operator declarations and may contain untrusted text;
the unchanged decision adapters treat state and descriptions as data.

## Result and authority

Authenticated advisory snapshots add `candidate_kind`. Profile-mode results
include optional `selected_profile` metadata and `selected_provider` for its
registered launch; probabilities are keyed by profile IDs. Provider-mode results
omit selected_profile and retain provider-keyed probabilities. Unknown/invalid
selections still fail through the host resolver, with explicit review/fallback.
Browser labels use literal text. Result metadata stays in memory, not event storage.

This mode can make experimental recommendations from declarations; it does not
establish effective model settings or routing quality. Confidence is not a task
success rate. The active profile, default configuration, prompts and permissions
never change as a result. Existing remote consent, deadline, request cap and
idempotency remain in force. Jev's upstream bounded retries still apply.

The existing candidate fingerprint covers candidate IDs/descriptions, not all
profile state. A future activation evaluation must bind full profile configuration
and capability evidence explicitly; it cannot reuse that fingerprint alone as
model/effort verification. No automatic dispatch or result reuse across restarts
is enabled here.

## Validation and next gate

Offline tests capture resolver input, filter unavailable profiles, check absence
of launch arguments/paths and map typed results. An empty profile catalog fails
before durable state. Browser acceptance checks a real offline recommendation,
inert label rendering, and unchanged active profile/conversation. Remote model
quality and effective model/effort verification remain untested in this slice.
A new frozen profile evaluation and ADR 0015 eligibility evidence are required
before connecting advice to automatic execution.

Primary documentation consulted on 2026-10-02:
[API](https://docs.typesafe.ai/api),
[intent routing](https://docs.typesafe.ai/patterns/intent-routing), and
[state](https://docs.typesafe.ai/concepts/state). The API model alias in examples
is not adopted; Urdia retains its existing pinned Jev adapter configuration.
