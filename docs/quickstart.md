# Your first Urdia task

Start here if you have not installed Urdia before. This is a **source preview**:
you build it on your computer. The reviewed baseline is **Ubuntu 24.04 x86_64**,
including Ubuntu inside Windows WSL. Native Windows installation comes later.

You will first open the interface without an AI account, then connect a coding
agent and check a small real task. Urdia does not include an AI subscription.

## 1. Open Ubuntu and prepare the build tools

On Windows, open your Ubuntu 24.04 WSL terminal. All commands below run inside
Ubuntu, not PowerShell. Keep the checkout in your Linux home directory.

```sh
sudo apt-get update
sudo apt-get install -y git curl python3 build-essential pkg-config libsqlite3-dev
```

Check whether Rust's installer is already available:

```sh
rustup --version
```

If that command is not found, install Rust using the
[official Rust instructions](https://rust-lang.org/tools/install/):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

The Rust command downloads and runs its official installer and adjusts your
user environment. Urdia's assistant does not run it or install system packages.

## 2. Download Urdia and check prerequisites

The public repository requires no GitHub login:

```sh
mkdir -p "$HOME/projects"
cd "$HOME/projects"
git clone https://github.com/SaulineytorX/urdia.git
cd urdia
rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy
python3 -I -B scripts/setup-urdia.py --check
```

Expected: **Prerequisites ready. No accounts or credentials inspected.**
If you already have this checkout, use the update steps below instead of cloning
over it. Downloads are needed for Rust and the first locked Cargo build.

## 3. Open the offline interface

```sh
mkdir -p "$HOME/projects/urdia-preview-project"
python3 -I -B scripts/setup-urdia.py \
  --workspace "$HOME/projects/urdia-preview-project" --demo
```

Answer **y** to build and **y** to start the local workspace. The first build can
take several minutes. Open the private URL printed by the terminal in your
browser; on WSL, use your Windows browser. Keep this terminal open.

Send `Hello`. Expected: the **Offline demo (not AI)** agent echoes the message.
This checks the interface only; it does not solve tasks or edit your project.
Stop the session with **Ctrl+C in its terminal** before continuing.

## 4. Connect a coding agent

Install and sign in to a supported coding agent separately, in this same
Ubuntu/WSL environment. Verify that a small request works in its own interface.
A successful login on a different computer does not connect this installation.

- **Qwen Code:** see [provider setup](providers.md). The reviewed CLI is 0.24.7;
  finding its executable does not confirm account access or quota.
- **Codex:** the experimental bridge currently requires **Codex CLI 0.159.2**.
  Other versions are refused. See [Codex setup](codex.md); do not downgrade an
  existing CLI blindly.
- **Other ACP agents:** see [provider authoring](provider-authoring.md).

Return to Urdia's source folder and run setup without `--demo`:

```sh
cd "$HOME/projects/urdia"
python3 -I -B scripts/setup-urdia.py \
  --workspace "$HOME/projects/urdia-preview-project"
```

Choose your available agent, confirm the build and start, then open the new URL.
The header should name your real coding agent, rather than the offline demo.
Manual selection is the default. No Jev key is required for this first task.

## 5. Complete and check a small task

Use the empty preview project for this exercise. In **Setup guide → Your first
verified task**, review the sample, confirm the empty practice project and click
**Prepare first task**. This opens an editable draft in Conversation without
sending it. Review your chosen agent/routing and send it explicitly. You can also
paste the same task below:

```text
Create calculator.py with an add(a, b) function.
Create test_calculator.py using unittest with exactly these three checks:
add(2, 3) == 5, add(-2, 3) == 1, and add(0, 0) == 0.
Run python3 -m unittest -v and report the result.
Limit changes to these two files.
```

Review the tool permission requests for the operation you intend to allow.
Agent work can consume its provider quota. Agents run as your operating-system
user; Urdia is not a sandbox.

After completion, open **Check your project → Review checks and execution
permission**. Choose **My Python project · existing unittest tests**, authorize
the check, then select **Run selected checks**. Expected: **3 tests run, 0 failures,
0 errors**. This check executes project Python without an AI call and can have
side effects; inspect the files before authorizing it.

You can also confirm the result in another Ubuntu terminal:

```sh
cd "$HOME/projects/urdia-preview-project"
ls -l calculator.py test_calculator.py
python3 -m unittest -v
```

Expected: both files exist and the three tests end with **OK**. A completed agent
turn alone does not prove this. See [Python checks](python-project-checks.md)
for the scope and limits of the independent check.

## 6. Use your project and optionally connect Jev

Stop the preview session. Run setup again with the absolute Linux path to your
existing project, for example `--workspace "$HOME/projects/my-project"`. Start
with a small, reviewable task and your own appropriate tests.

For optional automatic profile choice, open **Decision engines → Jev**, connect
your own key with explicit test consent, configure exact execution profiles in
**Setup guide**, then select **Jev chooses and runs** for one submitted task.
The connection test and task decisions can consume quota; Jev does not grant
tool permissions. See [automatic Jev](automatic-jev.md) and
[credential storage](jev-credentials.md). Laya setup is separate and optional.

For a repeatable existing-project exercise, try the
[repair example](../examples/repair-name/README.md): fix one source file while
keeping its tests unchanged. [The paired pilot](repair-pilot.md) records what was
observed with manual and Jev selection.

## Update an existing checkout

Stop Urdia first, then:

```sh
cd "$HOME/projects/urdia"
git status --short
```

If this reports local changes, preserve them before updating. Otherwise:

```sh
git pull --ff-only
python3 -I -B scripts/setup-urdia.py --check
python3 -I -B scripts/setup-urdia.py \
  --workspace "$HOME/projects/urdia-preview-project"
```

Confirm the build to use the updated source. The new session prints a new URL.
Do not use `--no-build` to update: it reuses a binary without verifying freshness.

## If something fails

| What you see | Next action |
| --- | --- |
| `rustup` or `cargo` not found | Install Rust above; reopen Ubuntu or load `$HOME/.cargo/env`. |
| Missing pinned Rust toolchain | Run the explicit toolchain install in step 2. |
| Missing SQLite development files | Install `libsqlite3-dev` in step 1. |
| `urdia: command not found` | Use setup from the source folder; no global installation is required. |
| Only Offline demo is listed | Install and authenticate a supported agent separately. |
| Codex version is unsupported | Review the bridge version in [Codex setup](codex.md); use another supported agent if needed. |
| Provider authentication error | Check a request in the agent's own interface; executable discovery is not authentication. |
| No project edits after sending a task | Confirm you are using a real agent and review pending permissions. |
| Zero tests discovered | Inspect the project path and `test_calculator.py`; zero tests is not success. |

For help, [report an issue](https://github.com/SaulineytorX/urdia/issues) with
your OS/WSL version, Urdia commit (`git rev-parse --short HEAD`), agent version,
failed step and sanitized error. Never include keys or the complete private
browser URL. Use the [security policy](../SECURITY.md) for suspected vulnerabilities.
