# Repair pilot: manual selection and Jev

On 2026-10-14, Urdia carried the same small existing-project repair through two
real coding-agent tasks. Both repaired normalize.py, preserved the existing tests
and passed separate checks. This was a synthetic exercise, not customer acceptance.
Try it yourself with [the repair example](../examples/repair-name/README.md).

## What was measured

| Condition | Chosen profile | Submit to terminal state | Jev decision | Existing tests | Additional cases |
| --- | --- | --- | --- | --- | --- |
| Manual default | Codex low | 5,870 ms | No decision call | 4 passed | 10 passed |
| Jev chooses and runs | Codex low | 7,941 ms | 93 ms | 4 passed | 10 passed |

Time covers submission through the terminal agent state, including selection and
scripted permission handling. Startup, the separate Jev connection test, output
verification and checker execution are excluded. These are observations from one
pair, not typical performance, a speed ranking or a model-only latency measurement.
Exact observations and hashes: [observed-pair.json](../examples/repair-name/observed-pair.json).

## Controls and limits

- Fresh byte-identical copies and the same fixed task were used for both attempts.
  The original four tests failed before each task (3 failures, 1 error).
- Default Codex-low remained eligible alongside Codex-medium; Jev was not forced
  to choose a different profile. Both used Codex CLI 0.159.2 and requested
  gpt-6.1-sol / low. Labels are agent-reported, not remote model attestation.
- Only normalize.py could be edited. Each task received one explicit one-shot
  edit approval. Tests stayed byte-identical; no trusted session or persistent
  allowance was activated. A test-only guard restricted edit paths; Urdia is not
  a general-purpose sandbox.
- Existing tests were checked through Urdia's Python-project check after agent
  completion. Ten additional cases were checked separately, after source review.
  They were not included in the coding task; this is not a frozen blind benchmark.
- No task was retried and no generated code was repaired by the operator. Both
  outputs had the same source hash. Two coding tasks, one Jev connection test and
  one Jev profile decision were used. Tokens and billing costs were not captured.
- Order was manual then Jev, once per condition. Network, generation variability,
  local scheduling and order can affect timing. Separate host execution metrics
  were not captured by this pilot recipe; the outer timing must not replace them.

## What this supports

Urdia carried one repair through manual selection and optional Jev selection to
results that passed unchanged project tests and additional checks. Jev chose the
same default profile, so this pair does not demonstrate better model selection,
quality improvement or cost savings. The 2,071 ms total-time difference cannot be
attributed entirely to the 93 ms decision or generalized from one pair.

Next evaluation: varied fixed tasks, repeated and alternating/randomized order,
all failures and refusals retained, fixed eligible profiles, and observed usage
when available. No reliability percentage or savings claim should be made from
this example. Automatic plan loops and production routing quality remain unproven.
