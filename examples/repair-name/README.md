# Repair an existing project without changing its tests

This intentionally broken, synthetic Python project gives you a repeatable repair
exercise. It is not customer production code or a routing benchmark. Build and
connect a real agent using [the quickstart](../../docs/quickstart.md) first.

## 1. Copy the broken project

From your Urdia source folder, in the same Ubuntu terminal:

```sh
mkdir -p "$HOME/projects"
repair_workspace="$(mktemp -d "$HOME/projects/urdia-repair-XXXXXX")"
cp examples/repair-name/normalize.py examples/repair-name/test_normalize.py "$repair_workspace/"
(cd "$repair_workspace" && python3 -I -B -m unittest discover -s . -v)
```

Expected: **4 tests run, 3 failures and 1 error**. This failure is deliberate.
The original strips the ends and lowercases text; it misses whitespace collapsing,
Unicode casefold and input validation. Each attempt must start from this same seed.
Keep the checker and manifest outside the agent's project.

## 2. Open the project with your agent

```sh
python3 -I -B scripts/setup-urdia.py --workspace "$repair_workspace"
```

Choose your real agent, confirm the build/start and open the private URL. The
offline echo demo cannot repair this project. Submit the following fixed task:

```text
Repair only normalize.py in this existing Python project. Preserve test_normalize.py byte-for-byte. normalize_name(value) must accept only str (otherwise TypeError), collapse whitespace using split/join, casefold for Unicode, and reject empty/whitespace-only strings with ValueError. The current complete normalize.py is:
def normalize_name(value):
    return value.strip().lower()

The existing unchanged tests cover repeated spaces, Straße->strasse, whitespace-only and integer input. Use no imports. Use file-edit tools only; do not execute commands/tests, read unrelated files, use network/MCP or delegate. If an approval is denied, stop without retrying. Do not claim tests ran: the operator will check independently. Keep the final response brief.
```

Review permissions for **normalize.py only**. Do not allow edits to the existing
tests. Agent completion and a correct repair are separate observations.

## 3. Check the unchanged tests and additional cases

After inspecting the change, choose **Check your project → My Python project ·
existing unittest tests**, explicitly authorize execution and run the check.
Expected: **4 tests passed**, with source hashes unchanged during checks.

The additional checker stays outside the agent's project. From the Urdia source
folder, explicitly run it after reviewing the repaired code:

```sh
python3 -I -B examples/repair-name/check-contract.py "$repair_workspace"
```

Expected: **10 tests passed**. It also refuses a changed protected test file using
fixture.json's original hash. These commands execute project Python as your user;
they are not a sandbox or a security assessment. No model call is used by checks.

## Optional: compare manual selection with Jev

Use a fresh copy for each attempt, the exact same task and checks, and your same
available profiles. Keep the manual default eligible for Jev. Connect Jev separately
and explicitly choose its experimental mode; keys never belong in the project.
Record failures, refusals and successful runs. Do not retry or edit generated code
silently. Record submit-to-terminal time separately from decision time and checks.
A same-profile selection is a valid outcome. Usage/cost stays unknown unless observed.

See [the first paired pilot](../../docs/repair-pilot.md). More varied tasks and
repeated, alternating order are needed before claiming savings or reliability.
