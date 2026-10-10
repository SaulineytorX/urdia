# Independent calculator acceptance verification

Urdia provides a confirmed dashboard action and a source-checkout Linux/WSL
helper for two reviewed tasks: `calculator-add-v1` and `calculator-five-v1`. It executes real workspace Python without consulting
an agent or decision engine. The dashboard checker is embedded in the binary; Python remains an optional
runtime prerequisite. Other tasks and languages are not covered.

## From the dashboard

Launch with `--verification-ui`. With the project launcher:

```sh
cd ~/projects/urdia
python3 -I -B scripts/start-project.py \
  --workspace "$HOME/projects/urdia-preview-project" \
  --verification-ui
```

Keep your existing provider/profile/engine flags when restarting. This option
requires Linux/WSL, `/usr/bin/python3` and `/bin/kill`; it is off by default.
After the latest task completes, open **Verify this calculator → Review checks
and execution permission**. Choose the acceptance contract, review its specification and the files, then check
the execution checkbox and click **Run
calculator checks**. The checkbox authorizes running project Python as your user;
it grants no ACP permission or trusted session. Opening the panel starts nothing.

**Prepare matching task draft** fills an empty composer with the selected contract's
requirements. It neither sends a task nor replaces an existing draft. Select the
contract before asking your agent to implement it; selection does not infer which
contract your project supports. Changing it clears execution consent. An uncertain
request keeps its original contract locked until its delivery is resolved.

Results show the selected host contract and the project's unittest suite separately,
with host durations and whether the two file hashes stayed unchanged. Failed,
cancelled or unavailable checks never appear as passed. **Stop checks** cancels
execution and waits for process cleanup. While a check is active, new tasks and
profile switches are blocked; drafting a prompt remains possible. Matching retries
reuse accepted work without another execution. A host allows 16 checks before
restart. No AI API call is made by verification.

A result is associated with the completed task but describes files at check time.
It does not prove the agent created those files. After a newer task starts, the
previous check is labeled **Historical result**. External edits are not watched
continuously. Evidence stays in authenticated host memory and disappears on restart.

## From the source-checkout terminal

After the agent finishes, stop editing the disposable preview project. Run:

```sh
cd ~/projects/urdia
python3 -I -B scripts/verify-calculator.py \
  --workspace "$HOME/projects/urdia-preview-project" \
  --confirm-execution
```

**Review the workspace before confirming.** This flag explicitly authorizes
executing its Python as your Linux user. The helper is not a sandbox: project
code can access your files or network and can modify files. Urdia does not
install dependencies or infer authorization from an agent's recommendation.
The helper omits inherited credentials from the child environment and uses a
temporary HOME; this does not prevent access to your account's files.

## What is checked

Two regular files must exist: `calculator.py` and `test_calculator.py`. Symlinks,
non-regular files, files over 1 MiB and invalid deadlines are refused. The helper
hashes these two files before and after execution. A detected change or removal
invalidates the result; this is not an inventory of other workspace changes.

Two separate Python processes run, with `-I -B` and no shell:

1. A host-owned unittest contract imports the API described below. The default
   `calculator-add-v1` checks `add(2, 3) == 5`, `add(-2, 3) == 1`, and
   `add(0, 0) == 0`.
2. A fresh process imports `test_calculator`, loads its unittest cases and runs
   them. Empty suites, skipped cases and ordinary test failures do not pass.

The default deadline is **10 seconds per check**, with a one-second termination
period. `--timeout` accepts 1–30 seconds. The existing Linux child-subreaper/pidfd
supervisor cleans descendants, including children left after the test parent
exits. Ctrl+C, TERM or HUP interrupts verification without a passing report.
Raw stdout/stderr is discarded, so project exceptions or printed credentials
are not included in the report. Run tests yourself separately if you need their
full diagnostic output and have reviewed the code.

## Five-number arithmetic

Choose **Five-number arithmetic · 12 test groups** in the dashboard, or add
`--contract calculator-five-v1` to the terminal command. Omitting `--contract`
preserves the existing two-number addition checks. Unknown names are refused
before workspace code executes; custom commands are not supported.

The reviewed Python API is `calculator.calculate(numbers, operation)`:

| Requirement | Behavior |
| --- | --- |
| Input | A list or tuple with exactly five finite Python `int`/`float` values; booleans excluded |
| Operations | Exact strings `add`, `subtract`, `multiply`, `divide`, evaluated left to right |
| Invalid container or element type | Raise `TypeError` |
| Wrong count, nonfinite number or unknown operation | Raise `ValueError` |
| Zero in any of the four divisor positions | Raise `ZeroDivisionError`; zero numerator is valid |
| Input preservation | Do not modify the supplied list; tuples produce the same result |

Twelve fixed groups cover the four operations, fractional/negative addition,
zero numerator, all divisor positions, invalid counts/types/nonfinite values/
operations, and input preservation with tuple equivalence. Division and the
fractional example use unittest's default seven-place approximate comparison.
The project suite runs separately and must contain successful, unskipped tests.
These are representative cases, not exhaustive proofs for all numbers, overflow,
error precedence, languages or a graphical calculator.

The submitted contract is bound to the request ID and completed task. Reusing an
ID with a different contract is a conflict; evidence naming a different contract
is discarded. Results keep their original contract label even if you change the
selector afterward. The same execution consent, deadlines, cancellation, file
hashes and same-user limitations apply to both contracts.

## Reading the result

The helper prints JSON with a schema version, contract name, overall status,
pre-check SHA-256 hashes, whether the two acceptance files stayed unchanged,
and each check's process status, exit code and host elapsed milliseconds.
New reports use schema version 2 and include an `unittest` aggregate: `tests_run`,
`failures`, `errors`, `skipped`, `expected_failures`, and `unexpected_successes`.
The dashboard displays these counts below each check. `tests_run` counts unittest
test methods, not individual assertions or subtest examples. Subtest failures can exceed
the number of test methods run; counts are not added together into a success score.
Expected failures follow unittest semantics and are displayed separately; skipped
or unexpectedly successful tests cannot pass acceptance.

A private, bounded summary file carries only these integer counters; raw project
stdout/stderr, test names and exception text are never parsed or retained. If an
import fails or execution times out before a summary is produced, counts may be
unavailable. A zero process exit without valid, nonempty successful counts is
refused rather than treated as a pass. Host counts must match the selected contract's
three or twelve test groups. The UI can read legacy version-1 reports with unknown
counts, but the current bundled runner requires version 2. Same-user workspace
code can still interfere deliberately; counts are observations, not attestation.
It writes no evidence file automatically. Do not confuse these observations
with provider token usage or a bill.

Exit 0 means both checks passed and the two file hashes stayed unchanged.
Exit 1 means acceptance failed; inspect `checks` and
`acceptance_files_unchanged`. Exit 2 means execution was not confirmed or inputs
were refused. Exit 130 means interrupted. A timeout is reported as a failed
overall acceptance with `timed_out` for the check.

A pass covers these specific examples and the project test suite at the time of
checking. It does not prove arbitrary calculator behavior, five-number UI
requirements, absence of changes elsewhere, future file contents or protection
against malicious workspace code. Workspace Python runs in the same account
and can deliberately interfere with a test runner or report: this helper is
independent of agent prose, not a security attestation. Use a trusted disposable
project and review the diff alongside the result.

Manual sending remains default. The [automatic Jev preview](automatic-jev.md)
is an explicit experimental per-task mode, not a passing production-quality gate
under [ADR 0015](adr/0015-automatic-routing-activation-gate.md).
Automatic task dispatch does not automatically authorize these checks.
The dashboard implements the fixed turn binding, operator confirmation, historical
result labeling and bounded cleanup in [ADR 0032](adr/0032-dashboard-calculator-verification.md).
The reviewed extension is recorded in [ADR 0033](adr/0033-reviewed-calculator-contracts.md).
General-purpose check commands and other task families remain separate work.
