# ADR 0010: Repeatable split routing evaluations

- Status: accepted
- Date: 2026-09-28
- Supersedes: none

## Context

The first Jev run selected an acceptable route for all 15 authored cases, but
one pass cannot measure consistency and one development corpus cannot support a
credible activation decision. A deterministic first-candidate router can also
be perfectly stable while consistently wrong, so quality and stability must be
reported separately.

## Decision

Keep the v1 fixture immutable and introduce corpus v2 with three explicit
partitions: 15 development cases, 6 held-out cases, and 6 adversarial cases.
Every candidate carries an explicit escalation label so evaluation does not
infer policy from a growing list of route names. The host validates case and
candidate IDs, exact split counts, acceptable routes, safety assertions, and
escalation-label consistency before invoking a decision provider.

`urdia eval-routing` accepts `--split` and `--repeats`. Repeats are sequential
and limited to ten because live runs consume provider quota and may be rate
limited. The default remains one run. Reports include sanitized per-run
observations and aggregate counts and rates globally, by split, and by risk.
They separate acceptable-route accuracy, selection coverage, semantic
abstention, operational failure, escalation quality, confidence, and exact
outcome stability.

Exact stability is the dominant outcome frequency for each case across its
runs. A change between two acceptable routes still counts as instability.
Objectives, state, candidate descriptions, raw provider responses, and
credentials never enter the report.

## Consequences

- Prompt and policy changes can be developed on one partition and checked
  independently on held-out and adversarial cases.
- A stable but inaccurate router remains visibly inaccurate.
- Operational outages cannot count as semantic escalation or coverage.
- Live repeated runs have an explicit, bounded cost multiplier.
- Results remain descriptive. They do not start agents, grant permissions, or
  activate trusted sessions.

## Validation

- Scripted-provider tests alternate valid choices, semantic abstention, and
  provider failure across repeats.
- Deterministic tests verify exact stability independently of accuracy.
- Checked-in v1 and v2 fixtures are validated for count, split, IDs, labels,
  and safety assertions before evaluation.
- Report tests assert that private input and provider error text are absent.
