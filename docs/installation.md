# Start Urdia on Linux or Windows with WSL

**First installation?** Start with [Your first Urdia task](quickstart.md):
prerequisites, public download, guided build, offline preview, real agent and checks.

This is a Linux/WSL source preview, not a prebuilt installer. Use Ubuntu 24.04 x86_64;
on Windows, run these steps inside Ubuntu on WSL. Install Rust with rustup first.
The checked-in toolchain file selects the reviewed Rust version.

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libsqlite3-dev python3
cargo build --release --locked
./target/release/urdia doctor
```

Run from the extracted source root. Doctor checks executable availability without
verifying sign-in, credentials or provider quota.

## Guided first run

After installing prerequisites, run the [guided setup](guided-setup.md) from the
source folder:

```sh
python3 -I -B scripts/setup-urdia.py
```

It asks before building and starting, offers an offline demo or a supported installed
agent, and keeps setup settings outside your project. No keys are entered here;
Jev configures in the browser. The manual path below remains available.

## 1. Try the browser without an AI account

```sh
mkdir -p "$HOME/projects/urdia-preview-project"
python3 -I -B scripts/start-project.py \
  --workspace "$HOME/projects/urdia-preview-project" --demo
```

Open the private URL printed in the terminal using your browser. Keep the terminal
open; Ctrl+C stops it. The offline demo echoes messages, uses no AI and edits no
project files. The UI is local, with no sensitive port opened to the network.

## 2. Use your coding agent

Install a supported agent separately and sign in through its normal interface.
For Qwen, confirm your account works in its own terminal first. Then:

```sh
python3 -I -B scripts/start-project.py \
  --workspace "$HOME/projects/urdia-preview-project" \
  --provider qwen --verification-ui
```

Give it one concrete task. Review each permission request, inspect the files and
explicitly run an appropriate check. Use **Setup guide** for configuration and
**Project plan** for reviewed steps. A project-directory or account change is not
inferred from a screenshot. See [providers](providers.md), [workspace](workspace-ui.md)
and [Python checks](python-project-checks.md). Agents/checks are not sandboxed.

Other agents require reviewed launch/profile configuration. Optional decision
engines advise; [automatic Jev selection](automatic-jev.md) is an optional
experimental mode for one submitted task. Keep API keys in the selected
agent/host's private configuration, never in project files, tasks or screenshots.

## 3. Connect Jev and optionally let it choose the profile

Open **Decision engines → Jev** in the workspace. Paste your own API key and
explicitly authorize **Connect and test**. This small fixed connection test can
consume provider quota; no project files or task text are included. No terminal
key entry or restart is needed.

Leave **Remember in this computer's credential store** unchecked for a
session-only connection. When a supported OS store is available, you can choose
Remember and explicitly restore the saved key after restarting. No plaintext
fallback is created. Linux GNOME storage has isolated native acceptance; Windows
Credential Manager through WSL remains unverified in this preview. Session-only
connection does not require a vault. See [Jev credentials](jev-credentials.md)
for storage requirements and privacy limits.

Connection tests are separate from task execution. To enable automatic selection,
create exact model/effort profiles in **Setup guide**, apply the indicated restart,
then choose **Jev chooses and runs** in **Conversation → Who chooses the agent?**.
Submitting authorizes that task's remote decision and starts the valid selected
profile without another selection confirmation. Tool permissions stay separate.
The mode is experimental and resets to manual on reload; keep the tab open until
task admission. See [automatic Jev](automatic-jev.md) for refusal/retry limits.

## 4. Check the result

After the task finishes, inspect its files and use **Check your project**. Python
checks need an applicable unittest suite or calculator contract. Optional static
frontend checks need Playwright and a separately installed Linux Chromium with a
working sandbox; they do not start React/Vite/build servers. See
[frontend checks](frontend-checks.md) before installing that optional runtime.
No verification or demo call uses a model. Agent and Jev calls can consume quota.

## Which Urdia version starts?

When running from source, the project launcher prefers this source tree’s
`target/release/urdia` and `target/release/urdia-fake-agent` over an older installed
version on PATH. An explicit `--urdia` or demo `--fake-agent` path takes precedence.
Only when the local build is absent does it look for an installed executable.
An existing unusable local build produces an error; rebuild it instead of silently
starting a different version. Stop the current session before launching an update.
