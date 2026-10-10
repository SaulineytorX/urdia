# Windows private preview with WSL

The first PC preview uses Ubuntu 24.04 on WSL 2 and a Windows browser. This is
not a native installer or a public release. The launcher is verified on our
Ubuntu server through real authenticated HTTP requests; Windows interoperability
still requires the owner's PC test. Manual sending is default. The optional
[automatic Jev preview](automatic-jev.md) can select a configured profile and run
one submitted task; the production-quality gate in
[ADR 0015](adr/0015-automatic-routing-activation-gate.md) remains unpassed.

## Prepare Windows once

In PowerShell, inspect installed/available distributions and install Ubuntu
24.04 if needed:

```powershell
wsl --list --verbose
wsl --list --online
wsl --install -d Ubuntu-24.04
```

Installation may require administrator permission and a restart. Open Ubuntu,
create your Linux user and run the remaining commands in its terminal.
[Microsoft's WSL guide](https://learn.microsoft.com/en-us/windows/wsl/install)
documents prerequisites and troubleshooting.

Use a separate WSL project checkout for the first agent task. Linux tools work
best with files under the Linux home directory; Windows paths are accessible
as `/mnt/c/...`. [Microsoft's filesystem guide](https://learn.microsoft.com/en-us/windows/wsl/filesystems)
explains path and performance differences. Spaces are supported; quote paths.

## Guided setup after prerequisites

Inside the source folder, run `python3 -I -B scripts/setup-urdia.py`. The
[assistant](guided-setup.md) checks prerequisites, asks before compiling, chooses
a project/agent and prints the private browser URL. It does not install WSL or
collect keys. Manual preparation and launch remain below.

## Get the private source and build

Inside Ubuntu:

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libsqlite3-dev python3 git curl
mkdir -p "$HOME/projects"
cd "$HOME/projects"
git clone git@github.com:SaulineytorX/urdia.git
cd urdia
```

Configure Git access for your own PC's GitHub account. Do not copy the server's
private SSH key. If cloning reports permission denied, arrange account access
before continuing. Follow the [installation guide](installation.md) to install
Rust through official rustup instructions if needed. The checkout pins Rust
1.98.1; build both executable programs:

```sh
cargo build --release --locked
./target/release/urdia doctor
mkdir -p "$HOME/projects/urdia-preview-project"
```

The first build downloads toolchain/dependencies and takes time. Session startup
reuses the compiled binary; it does not rebuild for each prompt.

## First: an offline visual demonstration

```sh
python3 -I -B scripts/start-project.py \
  --workspace "$HOME/projects/urdia-preview-project" --demo --check
python3 -I -B scripts/start-project.py \
  --workspace "$HOME/projects/urdia-preview-project" --demo
```

Open the exact private `http://127.0.0.1:.../#token=...` URL printed by Urdia in
your Windows browser. Keep the Ubuntu terminal open. Type a short message, watch
the response stream and send a second message. Ctrl+C stops the session.

The demo is a deterministic echo, labeled as not AI: it edits no files, requires
no model key and makes no paid inference call. It checks session transport,
not coding ability or model routing quality.

Windows normally accesses WSL network apps through localhost; see
[Microsoft's networking guide](https://learn.microsoft.com/en-us/windows/wsl/networking).
If connection fails, check that the Ubuntu process still runs and use the exact
URL. Investigate WSL localhost forwarding/update settings rather than opening
firewall ports or binding to `0.0.0.0`. The URL grants session access; do not share
it in screenshots or reports.

## Then: one real, checkable task

Install and authenticate your chosen ACP agent **inside WSL**, following its
[official Qwen quickstart](https://github.com/QwenLM/qwen-code/blob/main/docs/users/quickstart.md)
or the chosen provider's instructions. Urdia discovers Qwen Code with `qwen --acp`; finding
an executable does not establish authentication or local model inference.
An installed Qwen CLI does not prove local Qwen weights are available. Provider
usage may consume its quota. This launcher never invokes Jev or switches models.

```sh
qwen --version
./target/release/urdia providers
python3 -I -B scripts/start-project.py \
  --workspace "$HOME/projects/your-test-project" --provider qwen --check
python3 -I -B scripts/start-project.py \
  --workspace "$HOME/projects/your-test-project" --provider qwen
```

`--check` starts no agent and writes no session data. For real providers it
checks discovery, not credentials or model responses. Normal launch starts the
selected agent session. External ACP providers use `--config` with `--provider`,
or a named `--profile` / configured default. See
[execution profiles](execution-profiles.md) for selection and the visible profile
summary. Declared model/effort labels are not verification of effective settings.

For the first real prompt use a disposable project with a known failing test:

> Inspect this project. Explain the failing test, make the smallest fix, run the
> relevant test and show which files changed. Ask before destructive actions.

Inspect the actual diff and test result in Ubuntu. A fluent response alone is
not acceptance. One-shot permissions are handled in the browser; this launcher
does not enable a trusted session. It is not a sandbox: real agents run as your
Linux user and manage their own environment, authentication and storage.

## Data and cleanup

Urdia metadata lives outside the project under
`${XDG_STATE_HOME:-$HOME/.local/state}/urdia/projects/<workspace-and-selection-hash>`.
The final session directory is mode 0700. Demo and real histories are separate.
Urdia leaves prompt/response persistence off; provider-managed records are a
separate concern. Demo configuration contains executable paths, not credentials.
If its executable changes, stop the session, review and remove that directory's
`demo-provider.toml` before restarting.

The launcher replaces itself with Urdia so Ctrl+C reaches the host. After stopping
it, remove only the disposable project and its identified session directory if
desired. There is no background service, firewall change, startup registration,
credential copying or automatic dependency installation.

## PC acceptance checklist

- Record Windows/WSL/Ubuntu versions and the Urdia commit.
- Open the private URL and complete two demo turns without a paid call.
- Confirm cancellation visibly completes; restart after Ctrl+C.
- Verify that a URL without its token cannot access conversation data.
- Complete one real task; inspect the actual diff and passing test.
- Exercise a one-shot permission and a rejected request with a disposable task.
- Record startup/first-response timing and narrow/wide browser usability.
- Share only sanitized errors/screenshots without private URLs or project text.

Server automation verifies authenticated HTTP, two demo turns, argument handling,
readiness without writes, protected external state and content privacy. The PC
items remain unverified until run there. This guide does not certify Windows
support, additional real agents or automatic routing. Next: verify a second real
agent, make advisory routing visible, then evaluate a newly frozen profile before
automatic activation.

## Optional browser recommendations

For an installed Laya service inside WSL, connect it in **Decision engines → Laya**
using the local address and service key; no terminal variables or restart are
required. The connection is remembered for this Urdia process only. An installed
service on your remote server is separate from WSL.

Use `--advisory-ui` with `scripts/start-project.py` for host-configured engines
such as Jev. Configure those engines in the same host terminal before launching;
see the [workspace guide](workspace-ui.md#choosing-decision-intelligence) for the
source-build path and separate private-key input steps. Opening the view makes
no model call. Jev requires remote consent and may consume quota. A recommendation
does not start or switch the agent. `--check` remains a no-write readiness check.

## Verify the first calculator task independently

After the real agent creates calculator.py and test_calculator.py, use the
[calculator acceptance verifier](verification.md) from your source checkout.
It requires explicit permission to execute workspace Python, checks host-owned
examples separately from project tests and returns a redacted result. This is
available in the terminal or the dashboard with --verification-ui; see the
linked guide for explicit execution confirmation and check-time result scope.

### Five-number acceptance task

With `--verification-ui`, select **Five-number arithmetic** in the verification
panel and choose **Prepare matching task draft**. Review/send the draft to your
configured agent, then review its files and confirm independent checks. See
[the exact Python API and twelve test groups](verification.md#five-number-arithmetic).
This verifies calculation logic; graphical calculator acceptance is separate.

After updating/rebuilding, you can rerun **Run calculator checks** against the
existing generated files once the current session has a completed turn. If a
restart has no completed turn, a read-only agent request can establish that turn;
verification itself makes no AI call. The source-checkout terminal alternative
does not require a turn:

```sh
python3 -I -B scripts/verify-calculator.py \
  --workspace "$HOME/projects/urdia-preview-project" \
  --contract calculator-five-v1 --confirm-execution
```

Review the workspace before executing its Python. New reports show actual unittest
method counts, errors and skips separately from the agent's claims.

## Which Urdia version starts?

When running from source, the project launcher prefers this source tree’s
`target/release/urdia` and `target/release/urdia-fake-agent` over an older installed
version on PATH. An explicit `--urdia` or demo `--fake-agent` path takes precedence.
Only when the local build is absent does it look for an installed executable.
An existing unusable local build produces an error; rebuild it instead of silently
starting a different version. Stop the current session before launching an update.
