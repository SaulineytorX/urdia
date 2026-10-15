# Fixed routing sample

Two small existing-project tasks exercise interval merging and dependency ordering.
They extend the [single repair example](../repair-name/README.md); they are not
representative product benchmarks or blind evaluation cases.

Use a disposable practice directory. Copy the selected entry's `seed` to
`solution.py` and `tests` to `test_contract.py`, then submit its exact `task` from
[cases.json](cases.json). Allow edits only to `solution.py`. The public unit tests
are readable but write-protected. Additional property checks remain outside the
agent workspace; they are public fixtures too, not secret holdouts. Do not give
agents a copy of this checkout's checker calibration references.

After reviewing the resulting source, run the explicit checker:

```sh
python3 -I -B examples/routing-sample/check.py intervals /path/to/practice-directory
# Or select: dependencies
```

This executes project Python under your current account on Linux. A conservative
AST screen, child deadlines and resource limits reduce accidental evaluation
failures; they do not provide a security sandbox. The directory must contain
exactly the two regular files. Tests must match the fixture byte-for-byte; source
and test hashes must stay unchanged during checks.

Offline checker calibration (no model calls):

```sh
python3 -I -B examples/routing-sample/test_checker.py
```

Both broken seeds must fail; reference implementations must pass; modified tests,
symlinks, extra files and prohibited dynamic execution must be rejected.

See [the protocol and observed sample](../../docs/routing-sample.md) for ordering,
profile limits, all outcomes and what these measurements cannot establish.
