# Urdia routing evaluation corpus

This directory contains versioned, offline routing fixtures for Urdia. The
fixtures exercise semantic routing while keeping execution, permissions,
trusted-session activation, and network access under host control.

v1.json and schema.json are historical artifacts and remain unchanged.
v2.json and schema-v2.json are the Jev profile of the current development
corpus. v2-laya.json and schema-v2-laya.json contain the same 27 cases with
the historical typed Laya metadata. v2-laya-english.json and its schema use the
same cases for the isolated English checkpoint. v3.json and schema-v3.json
extend the Jev profile with paraphrase/generalization cases. v4.json and
schema-v4.json are the separately frozen blind Jev corpus. The v2 and v2-laya
duplicated cases must remain equal.

Recorded live results:

- `jev-1.13.0-2026-09-28.md`: first single-run v1 result.
- `jev-1.13.0-v2-3x-2026-09-28.md`: three repeated held-out and
  adversarial v2 runs, with sanitized JSON reports under `results/`.
- `laya-0.3.20-typed-decisions-v2-3x-2026-09-28.md`: the equivalent
  three-repeat local Laya run. It is an experimental baseline and did not meet
  an automatic-routing bar.
- `laya-0.3.20-english-v2-3x-2026-09-28.md`: the isolated English
  checkpoint comparison. It improved adversarial safety but remains ineligible
  for automatic routing.
- `jev-1.13.0-v3-3x-2026-09-28.md`: the prepared-route prompt-tuning run over
  v3, with sanitized held-out and adversarial reports under `results/`.
- `jev-1.13.0-v4-3x-2026-09-29.md`: the first run against the separately
  frozen v4 corpus. Its stable trusted-revision miss keeps automatic routing
  disabled.

[ADR 0015](../../docs/adr/0015-automatic-routing-activation-gate.md) defines
the provider-specific activation gate and records why the current Jev, Laya,
and TypeLLM profiles remain ineligible for automatic routing.

The v3 prompt was iterated after inspecting intermediate v3 results. V3 is
therefore a regression suite, not an untouched holdout suitable for an
automatic-routing activation decision.

## v4 blind structure

The v4 corpus contains 24 new cases that were frozen before any semantic
provider saw them:

- 12 held_out cases for ordinary routing and explicit review boundaries;
- 12 adversarial cases for indirect instruction injection, encoded or
  obfuscated secret egress, destructive euphemisms, dependency provenance,
  workspace escape, public control surfaces, stale trusted scope, and mixed
  intent;
- no development cases that may be used to tune the current prompt.

V4 does not copy the v1-v3 objectives. It keeps the gold routes only in each
case's `expected` object, outside the state sent to the decision provider. The
model remains pinned to `jev-1.13.0`; IDs are case-local and opaque; every
candidate has an explicit escalation label; and every case states that the
result is advisory and cannot authorize work or activate trust.

The corpus, schema, validation code, tests, and this protocol must be committed
before the first live v4 run. Do not change v4 after inspecting a model result.
If a defect is found, preserve v4 and create a new corpus version.

## v2 structure

The v2 corpus pins TypeSafe Jev to jev-1.13.0 and contains 27 cases:

- 15 development cases copied from v1.
- 6 held_out cases for evaluation after prompt or policy changes.
- 6 adversarial cases for instruction injection, secret egress,
  destructive euphemisms, and conflicting scope or network policy.

Every case has a case-local opaque candidate ID in the form c_XXXXXX.
Candidate order and ID assignment are intentionally varied. Each candidate
also has an explicit boolean escalation field. For every acceptable route,
that field agrees with the case's expected.must_escalate label. The
remaining candidate flags describe the host review or fallback behavior of
those alternatives.

Expected labels stay outside input.state. The
semantic_only_authorization assertion remains advisory: a route cannot
activate or widen a trusted session, grant a permission, or bypass the
host's current scope, revision, expiry, and revocation checks.

The new cases cover expired and revoked trusted sessions, workspace/provider
mismatch, missing capabilities or evidence, candidate order and ID bias,
prompt injection in the objective, state, and candidate description,
disguised secret egress, destructive language, denied network access, and a
dirty workspace with mixed scope.

## v3 prompt-generalization structure

The v3 corpus keeps all 27 v2 cases unchanged and adds four cases for the two
stable conservative Jev misses observed in the repeated v2 run:

- 15 development cases copied from v2.
- 8 held_out cases, including two paraphrased missing-evidence requests whose
  specific clarification route should beat the generic no-match outcome.
- 8 adversarial cases, including two destructive euphemisms whose specific
  denial route should beat a read-only or permission-checked edit route.

The Jev adapter still sends one English Choice question with the same
`POST /v1/systemone`, pinned model, response validation, advisory boundary, and
host-owned authorization. Its route descriptions now distinguish specific
prepared routes from the generic `__urdia_escalate__` outcome. The prompt says
to prefer a specific clarification, denial, or review route when it fits and to
use the generic outcome only after no specific route fits.

## Offline validation

The JSON documents can be parsed without network access:

~~~bash
python3 -m json.tool evaluations/routing/v2.json >/dev/null
python3 -m json.tool evaluations/routing/schema-v2.json >/dev/null
python3 -m json.tool evaluations/routing/v3.json >/dev/null
python3 -m json.tool evaluations/routing/schema-v3.json >/dev/null
python3 -m json.tool evaluations/routing/v4.json >/dev/null
python3 -m json.tool evaluations/routing/schema-v4.json >/dev/null
python3 -m json.tool evaluations/routing/v2-laya.json >/dev/null
python3 -m json.tool evaluations/routing/schema-v2-laya.json >/dev/null
python3 -m json.tool evaluations/routing/v2-laya-english.json >/dev/null
python3 -m json.tool evaluations/routing/schema-v2-laya-english.json >/dev/null
~~~

The corpus shape check must verify all of the following:

- v2 has exactly 27 cases with 15 development, 6 held_out, and 6 adversarial;
- v3 has exactly 31 cases with 15 development, 8 held_out, and 8 adversarial;
- v4 has exactly 24 new cases with 0 development, 12 held_out, and 12
  adversarial;
- unique case IDs and unique candidate IDs within each case;
- every acceptable route is present in its case's candidate set;
- every acceptable candidate's escalation value equals
  expected.must_escalate;
- no candidate or expected label contains a real credential.

A project evaluator may be run against the fixture when the routing CLI is
available:

~~~bash
cargo run -p urdia -- eval-routing \
  --fixture evaluations/routing/v2.json \
  --router deterministic \
  --split held-out \
  --repeats 3

cargo run -p urdia -- eval-routing \
  --fixture evaluations/routing/v3.json \
  --router deterministic \
  --split held-out \
  --repeats 3

cargo run -p urdia -- eval-routing \
  --fixture evaluations/routing/v4.json \
  --router deterministic \
  --split held-out \
  --repeats 3
~~~

The deterministic run is a plumbing baseline; candidate order and IDs are
deliberately unsuitable as a routing heuristic. Held-out and adversarial
results should be reported separately from development results.

## Live evaluation and cost

A live Jev evaluation is opt-in and requires the pinned model plus an
explicit live acknowledgement:

~~~bash
cargo run -p urdia -- eval-routing \
  --fixture evaluations/routing/v2.json \
  --router jev \
  --live
~~~

Repeating the v3 live run uses the same explicit acknowledgement:

~~~bash
cargo run -p urdia -- eval-routing \
  --fixture evaluations/routing/v3.json \
  --router jev \
  --live
~~~

The frozen v4 run used the same explicit acknowledgement. Any diagnostic
repeat must keep held-out and adversarial partitions separate and retain only
sanitized reports; repeating v4 cannot turn it back into unseen evidence:

~~~bash
cargo run -p urdia -- eval-routing \
  --fixture evaluations/routing/v4.json \
  --router jev \
  --split held-out \
  --repeats 3 \
  --live
~~~

Each repeated live evaluation sends the case objective, state, and candidate
descriptions to TypeSafe and can consume API quota. Repeat runs should be
intentional, keep sanitized reports, and avoid persisting raw responses or
TYPESAFE_API_KEY. A live result is an evaluation artifact; it never starts
an agent, grants a permission, or activates a trusted session.

Laya is evaluated through the authenticated local service and therefore does
not accept `--live`:

~~~bash
sudo systemd-run --wait --pipe --collect \
  --property=LoadCredential=laya_key:/etc/urdia/secrets/laya-english.key \
  --working-directory="$PWD" \
  target/debug/urdia eval-routing \
  --fixture evaluations/routing/v2-laya-english.json \
  --router laya \
  --split held-out \
  --repeats 3
~~~

The report keeps semantic quality separate from consistency. Exact outcome
stability is the frequency of each case's dominant selection or fallback over
its repeats. A router can therefore be perfectly stable and consistently
wrong. Reports include raw counts and rates globally and by split and risk
band, while omitting objectives, state, candidate descriptions, and raw
provider responses.

## Synthetic execution profiles v5

`v5-profiles.json` and `schema-v5-profiles.json` freeze 24 new profile selection
cases (8 development / 8 held-out / 8 adversarial). They retain the Jev pin and
existing adapter prompts. V5 reports bind full context, and
`v5-profiles-freeze.json` binds source/input/report artifacts. The initial Jev
one-pass development checkpoint accepted 8/8; subsequent reserved results are
recorded below. Other engines remain unmeasured. [Contract and limitations](../../docs/profile-evaluation.md).
Automatic routing remains disabled under ADR 0015.

The subsequent unchanged Jev reserved checkpoint accepted 7/8 held-out and 7/8
adversarial cases, including one host-ineligible unsupported-model selection.
[Analysis](jev-1.13.0-v5-profiles-reserved-1x-2026-10-02.md) and
[provenance](v5-profiles-reserved-checkpoint.json) preserve the failures.
Observed v5 is now regression evidence; its results and insufficient activation
coverage keep automatic routing disabled. No prompt/threshold/gold tuning or
additional paid repetitions were made.
