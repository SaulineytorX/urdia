# ADR 0012: Jev prepared-route preference and paraphrase evaluation

- Status: accepted
- Date: 2026-09-28
- Supersedes: none

## Context

The repeated Jev v2 evaluation was operationally stable but conservatively
selected the generic `__urdia_escalate__` outcome for two prepared routes: a
missing-evidence clarification and a destructive euphemism denial. Those cases
are semantically close to other routes, so the prompt needs to distinguish the
specific prepared route from the generic no-match outcome and test whether the
behavior generalizes across paraphrases.

## Decision

Keep v1 and v2 fixtures, schemas, and reports immutable. Add Jev corpus v3 by
copying all 27 v2 cases and adding exactly four cases: two held-out
missing-evidence paraphrases and two adversarial destructive-euphemism
paraphrases. The v3 split is 15 development, 8 held-out, and 8 adversarial.

Update only the English `Choice` instructions and string criteria in the Jev
adapter. The question remains one narrow Choice over the host-prepared route
IDs, with the same `POST /v1/systemone` request, pinned `jev-1.13.0` model,
response validation, and `__urdia_escalate__` option. The wording tells Jev to
prefer the most specific prepared route that fits, to distinguish
clarification, denial, and review routes, and to use the generic no-match option
only when no specific route fits.

The decision remains advisory. The host validates every selected ID and owns
execution, trusted-session activation, scope checks, and permissions. No live
v3 result is treated as an activation gate. Prompt wording was iterated after
inspecting intermediate v3 results, which makes v3 a regression suite rather
than an untouched holdout. A later activation decision requires a new blind
corpus frozen before its first live run.

## Consequences

- v2 remains a reproducible historical baseline for measuring the prompt change.
- v3 tests semantic paraphrases without changing candidate, transport, or
  authorization contracts.
- The evaluator must reject any v3 corpus whose metadata, case count, split
  counts, labels, or safety assertions are wrong before contacting Jev.
- A live v3 run is opt-in and can consume TypeSafe quota; its results remain
  descriptive until a later activation decision.

## Validation

- Focused Jev tests inspect the prepared-route contrast and generic no-match
  wording while preserving the request and response shape.
- Offline CLI tests validate checked-in v2 and v3 corpora and reject wrong v3
  counts or split counts.
- The final repeated v3 live run selected acceptable routes in all 48 recorded
  decisions with 100% operational success. Adversarial exact stability was
  100%; held-out exact stability was 87.5% because one case alternated between
  two acceptable routes.
- `cargo fmt --all -- --check`, targeted tests, and the workspace checks remain
  required before any live evaluation.
