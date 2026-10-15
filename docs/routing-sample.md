# Varied routing sample

## Frozen protocol

This small exploratory sample compares the explicit default profile with the
existing opt-in single-task Jev execution path on two fixed Python repairs:
interval merging and deterministic dependency ordering. It does not clear the
production routing gate, establish an optimal model, or estimate savings.

Before any task call, freeze [the fixtures](../examples/routing-sample/cases.json)
and their hash in `examples/routing-sample/protocol.json`. Each task has two paired
repeats, ordered manual/Jev then Jev/manual: eight planned rows in fresh workspaces
and agent sessions. Same seed, protected tests and exact prompt per task across
conditions. No replacement tasks, retries, operator source fixes, threshold tuning
or forced non-default selection after observing results.

The fixed eligible catalog is **Codex gpt-6.1-sol low and medium**, with low always
eligible and used as the manual default. These are requested controls, not
independently attested remote inference. This evaluates effort-profile selection
within one model/provider, not choices among multiple providers. No measured
price/quality ranking is supplied in candidate metadata.

Budgets are at most eight coding submissions, four Jev routing jobs and four
separate small connection checks. Permission approvals have their own cap of four
edit batches per task; they are not Jev decisions. All edit targets must be exactly
`solution.py`, enforced by the ACP tool guard. No execution tools, automatic trusted
session, permission grants by a model, network tools or delegation are allowed.
Urdia and this guard are not operating-system sandboxes.

Tests are public, agent-readable unit tests protected against writes. Property
checks are outside the task workspace but published here. Tests are deterministic,
not AI judgments. AST-screen source before explicitly executing Python; compare
file hashes before/after checks. Eight unit tests per task plus 2,352 interval
property assertions or 128 graph cases (including cycles and map-order changes).
Checker calibration rejects both original seeds, accepts known-good references,
and rejects changed tests, unexpected files, symlinks and prohibited constructs.

Retain every planned row, including refusal, timeout, check failure or scope denial.
Credential connection failure stops remaining paid calls and marks remaining rows
not run. A refusal counts against end-to-end success, not as a missing observation.
No latency replacement for failed/refused rows. Report host execution/first-text
metrics separately from submit-to-terminal wall time and observed permission
handling delay. The latter starts when the polling client first sees a request,
so it excludes up to a poll interval; it is not total human decision time. Neither
wall time nor delay-adjusted time proves pure model latency. Startup and
independent checks are outside submit-to-terminal timing. Unknown usage and cost
remain null; do not infer savings from profile labels or timing.

## Results

The unchanged protocol completed eight of eight planned rows: manual **4/4** and
Jev **4/4** passed the project suite and additional checks. No refusal, timeout,
retry, source-scope denial, test modification or unrecorded replacement occurred.
Each run needed one approved source-edit batch. Four separate Jev connection
checks preceded four routing jobs; they are outside task wall timing.

| Row | Task | Repeat | Mode | Selected profile | Decision ms | Wall ms | Permission handling ms | Host execution ms | First text ms | Result |
| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | intervals | 1 | manual | codex-low | 0 | 9615 | 147 | 9434 | 2432 | passed |
| 2 | intervals | 1 | jev | codex-low | 79 | 12807 | 100 | 12222 | 3674 | passed |
| 3 | intervals | 2 | jev | codex-low | 171 | 10657 | 375 | 10151 | 3384 | passed |
| 4 | intervals | 2 | manual | codex-low | 0 | 9546 | 404 | 9457 | 1888 | passed |
| 5 | dependencies | 1 | manual | codex-low | 0 | 12519 | 380 | 12365 | 2337 | passed |
| 6 | dependencies | 1 | jev | codex-low | 86 | 13885 | 120 | 13296 | 3059 | passed |
| 7 | dependencies | 2 | jev | codex-low | 88 | 13848 | 286 | 13258 | 2988 | passed |
| 8 | dependencies | 2 | manual | codex-low | 0 | 13650 | 105 | 13454 | 2485 | passed |

Jev selected **codex-low in all four decisions**, matching the manual default.
Routing decisions took 79–171 ms. All outputs passed eight unchanged project tests
plus their task's property checks; the full interval/graph corpus was rerun for
each output. The sample shows successful end-to-end execution but **no demonstrated
selection advantage**. Only low was actually executed, so this cannot compare low
against medium quality or cost. Jev wall times were longer in both paired repeats
of both tasks; these few unblinded, approval-inclusive timings do not establish a
reliable performance difference. Usage and billed cost were not observed.

[Complete observed rows and source hashes](../examples/routing-sample/observed.json)
include the generated output files for offline replay. The protocol's source commit
identifies the private execution checkout; the report names the equivalent public
runtime source (3df25a2). New example/checker bindings were frozen before calls.
There are only two small task families and two repeats: this is not a representative
benchmark, blind generalization evidence or production-routing approval. Next,
collect diverse real-project outcomes and genuine provider/model alternatives
before changing routing defaults or making optimization claims.

Methodological review used a real, tools-disabled Gemini 4 Argon request. Root
retained the eligible default, clarified Jev calls versus edit approvals, fixed
read/write visibility wording, and added permission handling measurements. Exact
checks remain in code. This follows TypeSafe's distinction between a structured
choice and evaluated application correctness; see [confidence guidance](https://docs.typesafe.ai/confidence)
and [intent routing](https://docs.typesafe.ai/patterns/intent-routing).
