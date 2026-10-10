# ADR 0020: Frozen synthetic profile selection evaluation

- Status: accepted
- Date: 2026-10-02
- Extends ADR 0018/0019; ADR 0015 remains in force.

## Decision

A separate v5 corpus evaluates advice over execution profiles with 8 development,
8 held-out and 8 adversarial cases. Its strict synthetic state distinguishes
operator labels, launch availability/authentication, requested versus observed
controls, advertised options, launch/revision evidence, task trials, user pins,
default preference and unknown measurements. Fixture evidence is not production
attestation. Existing corpora and semantic adapter instructions remain unchanged.

The fixture-only checker evaluates deterministic eligibility separately from
semantic task fitness. It requires executable/auth readiness, supported requested
controls, matching observed controls and launch, current evidence, some task
trial evidence, and compliance with the operator pin. A default is a preference,
not a pin. The checker validates ground-truth consistency and reports whether a
raw selected candidate passes the host rules (or is a prepared review route).
It does not filter live browser advice, dispatch agents or authorize permissions.

V5 records full input and complete fixture fingerprints, independent of the
candidate-only compatibility fingerprint. JSON objects are recursively sorted,
arrays preserve order, compact JSON is hashed with SHA-256. This is a project
binding format, not standardized JSON canonicalization. A regression pins the
frozen fixture digest; a manifest separately hashes schema, sources and the
sanitized development result. These bindings identify tested artifacts, not the
truth of synthetic state, deployed binaries or live inference configuration.

## Evidence and remaining gates

A single owner-authorized real Jev development pass accepted 8/8 cases with no
ineligible selections. It used the protected existing credential in process
memory; no secret file was added to Urdia and no coding agent started. Exact HTTP
attempt/token/cost accounting is absent from this evaluator. No prompts were tuned
against this run. Reserved partitions were subsequently evaluated as recorded
below; other-engine v5 measurements remain pending.

This development result cannot clear ADR 0015 or establish task success rates for
actual agents. Before activation, evaluate reserved cases without tuning, bind
current launch/model/effort evidence, handle live configuration drift, validate
another real ACP adapter, recheck owner choices and preserve host permission
policy. Tuning requires new unseen evaluation cases. Scheduling needs independent
workspace ownership, dependencies, cancellation and recovery contracts.

Primary guidance consulted: [TypeSafe intent routing](https://docs.typesafe.ai/patterns/intent-routing),
[state](https://docs.typesafe.ai/concepts/state) and [API](https://docs.typesafe.ai/api).
Pinned `jev-1.13.0` is retained rather than adopting the examples' floating alias.
See [the evaluation guide](../profile-evaluation.md).

## Reserved outcome checkpoint — 2026-10-02

The unchanged frozen Jev adapter returned 7/8 acceptable held-out and 7/8
acceptable adversarial choices in one pass each. A held-out choice was unnecessary
operator review; an adversarial choice selected an unsupported requested model
and failed fixture host eligibility. Both partitions were operationally successful.
No coding agent or permission was activated; no labels, prompt or threshold changed.
See [the failure analysis](../../evaluations/routing/jev-1.13.0-v5-profiles-reserved-1x-2026-10-02.md).

Automatic routing remains disabled. This checkpoint fails semantic quality/safety
and also lacks ADR 0015's activation corpus size/repeats/coverage. All observed v5
cases are now regression evidence rather than reusable blind approval for tuning.
Keep the original freeze manifest immutable; the separate reserved checkpoint
binds the tested commit/binary, original manifest and result hashes. Continue with
host-observed live configuration drift, real adapter/task evidence and production
eligibility checks before a fresh preregistered activation evaluation.
