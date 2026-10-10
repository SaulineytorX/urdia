# Laya 0.3.20 routing evaluation — 2026-09-28

The local `typed-decisions` checkpoint was evaluated against the same 6
held-out and 6 adversarial v2 cases used for Jev, with three sequential runs
per case. The service used Laya 0.3.20, CPU-only PyTorch, and Hugging Face
revision `1a793eb568e6718f15941d08f85432581df534e3` behind authenticated
`127.0.0.1:18765`.

## Results

| Split | Decisions | Acceptable | Operational | Exact stability | Escalation precision | Escalation recall | Confidence range |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| held-out | 18 | 33.3% | 100% | 100% | 75% | 60% | 0.0130–0.0241 |
| adversarial | 18 | 33.3% | 100% | 100% | 75% | 60% | 0.0089–0.0301 |

Laya selected a route for every decision and produced the exact same outcome
on all three repetitions. That stability does not imply correctness: four of
six cases failed in each split.

Material held-out failures included selecting a trusted-policy route after
revocation, choosing a permission-checked edit for a workspace/provider
mismatch, and reading history when evidence was missing. Material adversarial
failures included choosing permission-checked edits after prompt injection and
destructive euphemism, and failing to select the explicit secret-egress block.

Choice confidence is normalized entropy in Laya and all observed values were
very low. The checkpoint also emitted an upstream warning that one high-option
calibration bucket contains an invalid temperature. Urdia therefore records
Laya probability semantics as `Unknown` and applies no shared Jev threshold.

## Decision

Laya remains available only as an explicit advisory experiment. It is not
eligible for automatic routing, trusted-session activation, or permission
decisions. The next iteration should test a prompt tailored to this checkpoint
or a fine-tuned Urdia model; it must improve held-out and adversarial safety
before any activation policy is considered.

The JSON reports persist only case IDs, opaque route IDs, fingerprints,
provider semantics, confidence, fallbacks, and aggregate metrics. They omit
objectives, state, candidate descriptions, raw responses, and credentials.
