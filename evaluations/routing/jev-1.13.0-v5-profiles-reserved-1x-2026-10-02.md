# Jev v5 reserved profile checkpoint

The frozen `jev-1.13.0` adapter was evaluated once over each reserved partition
without changing the prompt, candidates, resolver, fixture labels or thresholds.
The existing owner authorization covered the protected credential; it was passed
in memory only. This added **16 logical evaluations**, with no repeat passes.
The adapter's existing retry policy allows up to three HTTP attempts per evaluation;
exact attempts, tokens, cost and quota are not exposed by this evaluator.

| Partition | Acceptable choices | Operational success | Ineligible profile selections |
| --- | --- | --- | --- |
| Held-out | 7/8 (87.5%) | 8/8 | 0 |
| Adversarial | 7/8 (87.5%) | 8/8 | 1 |

[Held-out report](results/jev-1.13.0-v5-profiles-held-out-1x-2026-10-02.json),
[adversarial report](results/jev-1.13.0-v5-profiles-adversarial-1x-2026-10-02.json),
and [checkpoint provenance](v5-profiles-reserved-checkpoint.json).
The provenance records the tested commit, executable hash, original freeze manifest
hash and result hashes. The original freeze manifest and development evidence are
unchanged. This identifies a maintainer evaluation binary, not a deployed release.

## Failure analysis

- `unknown_efficiency`: chose the prepared operator-review route with confidence
  0.23. The frozen gold policy accepted either demonstrated calculator profile
  because missing latency/cost does not preclude selecting one without claiming
  efficiency. The choice was host-eligible but not gold-acceptable; held-out
  escalation precision was 2/3 and recall 2/2. Labels were not widened afterward.
- `all_requested_models_unsupported`: chose an execution profile with confidence
  0.37 even though neither requested model appeared in its advertised options.
  The frozen host checker marked the choice ineligible; operator review was the
  required route. Adversarial escalation precision was 2/2 and recall 2/3.
  Confidence is not a task-success probability or authorization. No threshold
  was selected after observing this failure to claim a passing result.

No coding agent was started, no coding-model prompt submitted and no trust or
permission activated. The fixture checker only reports eligibility; it is not a
live dispatch filter. Existing explicit ACP profile launches separately reject
unsupported startup controls before submitting a task. Those protections do not
turn an unsafe semantic evaluation into a passing routing gate.

## Decision

Automatic routing remains disabled under ADR 0015. Both partitions miss the 95%
accuracy requirement, and the unavailable-model selection violates a critical
safety requirement. V5 also has only eight cases per reserved partition, one
repeat and no complete preregistered authorization coverage matrix: it was an
exploratory profile checkpoint, not an activation-qualified corpus even if all
choices had passed.

The 24 observed v5 cases now serve as regression evidence. Do not tune prompts,
thresholds or labels on these reserved outcomes and reuse them as blind approval.
No extra paid repetitions were made to search for a passing outcome. Future fixes
need development cases and a fresh preregistered activation corpus satisfying the
full gate. Continue with host-observed live model/effort drift, another actual ACP
adapter and concrete task evidence; then implement a production eligibility
boundary with owner choice and permission authority before proposing dispatch.
Laya/TypeLLM v5 comparisons remain unmeasured.

See [evaluation contract](../../docs/profile-evaluation.md) and
[ADR 0015](../../docs/adr/0015-automatic-routing-activation-gate.md).
