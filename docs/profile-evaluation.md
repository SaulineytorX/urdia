# Execution-profile evaluation

The frozen v5 corpus tests advisory selection over synthetic execution profiles.
It does not benchmark real coding models, prove provider authentication or enable
automatic execution. All examples and recorded task/latency evidence are fixtures;
model names are deliberately fictional. User defaults/pins, startup control
compatibility and task fitness are distinct dimensions.

## Frozen inputs

[evaluations/routing/v5-profiles.json](../evaluations/routing/v5-profiles.json)
contains 24 new English cases: eight development, eight held-out and eight
adversarial. [The schema](../evaluations/routing/schema-v5-profiles.json) describes
`urdia.profile-routing-evaluation/v5` and its strict profile-state contract.
v1–v4 artifacts, adapter prompts and pinned Jev model remain unchanged.

The cases cover the five-number calculator, backend versus frontend tasks,
explicit pins, equivalent-profile defaults, missing executable/authentication,
unsupported model/effort, optional effort, missing task trials or measurements,
stale revisions, launch/configuration evidence mismatch, instructions hidden in
labels/objectives, permission mode confused with reasoning effort, and a compound
backend/frontend/test request that needs a plan before choosing one profile.
Both equally supported routes are acceptable when efficiency evidence is absent;
there is no invented speed or cost ranking.

The **fixture-only** host checker rejects profiles with unavailable executable,
unconfirmed authentication, unsupported controls, stale revision, mismatched launch
or observed controls, absent task evidence, or conflict with a user pin. A default
is a preference among otherwise equivalent eligible candidates; it is not a pin.
Task fitness still requires a semantic judgment. This checker is not installed
in the browser advisory flow or live dispatch. Synthetic authentication/evidence
booleans are not a production attestation format.

## Run an evaluation

From a built source checkout:

```sh
./target/release/urdia eval-routing \
  --fixture evaluations/routing/v5-profiles.json \
  --router deterministic --split development --repeats 1
```

The offline deterministic adapter only verifies the pipeline; its results do not
measure semantic model quality. A real Jev development run requires explicit
remote opt-in and your key in the same terminal:

```sh
read -rsp 'Jev API key: ' TYPESAFE_API_KEY
printf '\n'
export TYPESAFE_API_KEY
./target/release/urdia eval-routing \
  --fixture evaluations/routing/v5-profiles.json \
  --router jev --live --split development --repeats 1
unset TYPESAFE_API_KEY
```

This requests eight logical evaluations. The existing adapter can retry each up
to three times; this command does not expose exact HTTP attempt counts, token usage
or cost. All state sent remotely is synthetic corpus data. No coding agents are
started, prompts submitted to coding models, trust activated or permissions granted.
An evaluation exits successfully when it produces a report, even if model quality
fails: inspect its metrics and case outcomes, not just its exit code.

Jev metadata is pinned to `jev-1.13.0`. Laya/TypeLLM need separately frozen matching
model metadata before comparison; changing a Jev fixture's router alone is rejected.
Their v5 quality remains unmeasured at this checkpoint.

## Provenance and reports

V5 reports add `fixture_fingerprint`, covering the complete parsed fixture
(including labels), and per-case `evaluation_input_fingerprint`, covering objective,
candidate IDs/descriptions/order and complete nested state. The project binding
recursively sorts JSON object keys, preserves array order and hashes compact JSON
with SHA-256. It is project-specific, not RFC 8785 canonicalization. The fixture
binding uses a fixed domain wrapper; its value is pinned in a regression test.
The previous candidate fingerprint remains for compatibility and is insufficient
for configuration reuse by itself.

`host_eligible_selection` says whether the returned candidate passes fixture host
rules or is a prepared review route. It is false for no selection or a forbidden
profile. It is a reported check, not a filter that rewrites raw semantic choices,
a permission grant, or an activation flag. Accuracy, abstention, operational
success and eligibility should be assessed separately. Reports omit objective,
raw state, labels, credentials and provider response bodies.

[The freeze manifest](../evaluations/routing/v5-profiles-freeze.json) records hashes
of the fixture, schema, evaluator, fixture policy, resolver and unchanged Jev
adapter, plus the sanitized development report. These are source/artifact hashes;
they do not bind a deployed binary, live provider inventory or real task evidence.
A future activation gate must bind those too and recheck current configuration,
freshness, user choice and host permission policy before dispatch.

## First development checkpoint — 2026-10-02

One real Jev pass accepted **8/8** development cases, with **0** ineligible
selections and **8/8** operational successes. It selected the prepared review
route for missing task evidence. Source prompt/model settings were unchanged;
there was no prompt tuning or repeated search over this result.
[The sanitized report](../evaluations/routing/results/jev-1.13.0-v5-profiles-development-1x-2026-10-02.json)
is checked in. The existing owner-authorized credential was read into memory
from the protected Price Signal credential file; it was not copied into a Urdia file,
printed, sent to the browser or committed. No provider processes or installed
services were modified. Exact quota consumption is not available in this report.

This is a small development result, not held-out accuracy or proof of safe
orchestration. At the development checkpoint, held-out and adversarial partitions had not yet
been sent to a model. Their subsequent result is recorded below. ADR 0015 remains in force. Reserved cases were subsequently evaluated without
prompt changes, as recorded below. Fixes must use development data and require
fresh unseen cases when tuning. Live configuration drift, another actual ACP adapter,
safe workspace scheduling and real task/latency/token/cost evidence remain separate
acceptance work before automatic execution.

## Reserved checkpoint — 2026-10-02

The unchanged frozen adapter accepted **7/8 held-out** and **7/8 adversarial**
cases in one pass each. Operational success was 8/8 for each partition. One
held-out choice unnecessarily requested review when either demonstrated profile
was acceptable; one adversarial choice selected a profile whose requested model
was unsupported. The latter was marked host-ineligible. No coding agent ran.

[Failure analysis and decision](../evaluations/routing/jev-1.13.0-v5-profiles-reserved-1x-2026-10-02.md)
and [provenance](../evaluations/routing/v5-profiles-reserved-checkpoint.json)
record the two sanitized reports, unchanged freeze, tested commit and binary hash.
This added 16 logical real Jev evaluations, no repeat passes. Exact quota/attempts,
tokens and cost remain unavailable. The key stayed in memory; installed services,
configuration and public listeners were untouched.

These outcomes fail quality/safety requirements. V5's size, single repeat and
coverage also make it insufficient for activation under ADR 0015. No gold labels,
thresholds or prompts were changed to improve the measured result. The observed
v5 corpus is now regression evidence; tuned policies require new unseen cases.
The next implementation slice is live ACP configuration reporting/drift and a
second actual adapter, then a production eligibility boundary grounded in host
observations. The fixture checker is not that production boundary. Automatic
routing remains disabled, and Laya/TypeLLM v5 evaluations remain pending.
