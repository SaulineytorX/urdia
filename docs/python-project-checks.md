# Check an existing Python project

After an agent task finishes, open **Check your project → Review checks and
execution permission**. Under **What do you want to check?**, select **My Python
project · existing unittest tests**. Review the scope, authorize execution, then
select **Run selected checks**. **Stop checks** cancels a running check.

This uses the system Python standard library and discovers `test*.py` unittest
modules in the workspace. Nested test directories must be importable Python
packages, as with stdlib discovery. It installs no dependencies and runs no AI.
Projects that need pytest, a virtual environment or other runtimes are not covered
by this contract. Missing imports appear as errors rather than triggering installs.

At least one test must execute. Failures, errors, skipped tests, expected failures
and unexpected successes cannot pass. The UI shows aggregate counts and elapsed
time; it does not retain test names, exceptions, stdout or stderr. A pass records
the project's own test-suite observation. It is not a host-authored requirements
oracle or proof that arbitrary application behavior is correct. Project code can
influence its test framework; Urdia is not a security sandbox.

The checker fingerprints relative names and contents of eligible `.py` sources
before and after execution, returning only a digest and file count. Source additions,
removals or changes prevent a pass. Equal snapshots do not prove that a file was
never changed temporarily, or that other files were unchanged. Limits are:

- 128 Python source files, 1 MiB each, 8 MiB total;
- 2048 scanned directory entries and eight levels below the workspace;
- no source-file symlinks or non-regular files, and no traversed directory links;
- hidden paths, `.git`, `.venv`, `venv`, `__pycache__` and `node_modules` excluded
  from both fingerprinting and discovery;
- ten seconds for the check, plus the existing outer supervisor/cancellation bounds.

Unusable source layouts produce a fixed unavailable result, not a fabricated pass.
Raw project execution inherits a filtered environment and temporary HOME but still
runs as your operating-system user and can access files/network. Approval is for
this check only; no ACP permission or trusted session is granted.

The result binds to the completed host task. A later task marks it historical; a
retry cannot inherit the earlier result. [Project results](project-summary.md)
shows it separately from agent completion and operator review. The two fixed
calculator contracts remain available when their specific independent examples
are appropriate. See [ADR 0043](adr/0043-bounded-python-project-checks.md).
