# Native Codex (experimental)

Urdia can launch the installed **Codex CLI 0.159.2** through the optional
`scripts/codex-acp.py` bridge. This is a second coding agent, separate from
Jev/Laya decision engines. It uses Codex's existing authentication, including a
ChatGPT login; Urdia does not read, copy, or ask for that credential. Account
entitlements and usage limits still apply. Manual sending is default; [automatic Jev preview](automatic-jev.md) is optional.

The bridge currently requires Linux/WSL, Python 3.10+, and exactly that CLI
version. The app-server protocol is experimental: other versions fail closed.
Do not downgrade or replace a working CLI automatically. Installation and login
belong to Codex; see the [official app-server documentation](https://learn.chatgpt.com/docs/app-server).
Check `codex --version` and `codex login status` in the same Linux/WSL environment
where Urdia runs. A server login does not authenticate your Windows/WSL installation.

## Register and run

From the Urdia source checkout, create an external configuration. The following
writes only `codex-provider.toml`; it resolves executable/script paths rather than
putting shell expressions into TOML:

```sh
python3 -I -B - <<'PY'
import json, shutil
from pathlib import Path
codex = shutil.which('codex')
if not codex:
    raise SystemExit('Install and authenticate Codex first')
script = Path('scripts/codex-acp.py').resolve(strict=True)
python = shutil.which('python3')
Path('codex-provider.toml').write_text(
    '[[provider]]\nid = "codex"\nname = "Codex native (experimental)"\n'
    + 'command = ' + json.dumps(python) + '\nargs = '
    + json.dumps(['-I', '-B', str(script), '--codex', codex]) + '\n')
PY

./target/release/urdia --config codex-provider.toml doctor
./target/release/urdia --config codex-provider.toml chat \
  --provider codex --workspace "$HOME/projects/my-project"
```

Build the release binary first using [installation](installation.md). Use an
existing workspace directory. The private loopback URL opens the normal Urdia
conversation, metrics, tool activity and one-shot permission dashboard. Keep the
terminal open; stop it with Ctrl+C. Finding Python in `doctor` does **not** prove
Codex/version/login/model availability; those are checked on actual startup/use.

By default the bridge uses Codex's configured model and effort. To pin them, add
`"--model", "YOUR_ACCESSIBLE_MODEL", "--effort", "low"` to the argument array.
Choose an accessible model from your current native Codex installation. A model
catalog entry is not proof of account entitlement. Live session selectors contain
only the settings confirmed by app-server; changing settings in an existing
session is unsupported. Restart with explicit launch arguments to change them.
Native confirmation is not independent proof of which backend executed inference.

## Permission and privacy boundaries

The native thread requests `read-only` sandbox, `on-request` approval policy and
`user` approval reviewer; all three must be confirmed before any prompt. Only
correlated command and file-edit approval callbacks can become Urdia one-shot
requests. Urdia persists its existing redacted receipt before responding. No
session approvals, policy amendments, root/network grants or silent tool answers
are supported. Unsupported callbacks fail closed. Explicit native approvals can
allow execution beyond the initial read-only restrictions: Urdia is not a sandbox.
Only approve an operation you expect in the selected workspace.

Text is forwarded to the authenticated conversation. Tool reports forward category,
status and a private correlation ID into Urdia's existing redaction boundary;
commands, paths, diffs and output are not dashboard telemetry. Permission arguments
remain private to the host approval boundary. Reasoning and private native errors
are not forwarded. A failed native turn is an error, never a completed task.

The bridge requests ephemeral threads and does not store credentials or transcript
files. This does not control Codex's own logging, runtime records, provider retention
or external tools. Only text input, one thread and one active turn are supported.
Client MCP registration, images, interactive user-input callbacks and live model
changes are unsupported. Existing native CLI configuration may affect behavior;
this adapter does not certify third-party tools or isolate credentials.

Protocol frames, prompt length, active callbacks and per-turn telemetry are bounded.
Cancellation rejects outstanding permissions and interrupts the native turn,
including cancellation racing with startup. EOF/signals terminate and reap the
native process group. Errors deliberately omit private diagnostics.

## Verified scope

On 2026-10-02, native Codex 0.159.2 with the owner's ChatGPT account and
`gpt-6.1-sol`/`low` completed a real Urdia text task. A file-edit task was rejected
under default permissions and created no file. With one explicit one-shot edit approval, Codex created only `calculator.py`;
the host independently verified three addition cases. No execution approval
was granted. This validates a small file-edit task, not every native tool. Offline regressions exercise
protocol startup, streaming, permission scope, cancellation, limits and process
cleanup without account/network calls. Ordinary CI never runs paid Codex inference.

The external `@zed-industries/codex-acp@0.16.0` adapter was also evaluated but used
an older embedded runtime; it could not run the owner's current accessible model.
This bridge uses the installed native runtime instead, without copying third-party
adapter source or adding its dependencies.

## Optional real dashboard acceptance

`scripts/check-codex-browser.py` is a repeatable, explicitly paid/account-quota
check, separate from ordinary offline CI. It requires Playwright and an already
installed Chrome/Chromium. It creates a disposable workspace; it never runs in
an existing customer project. Run it only when you authorize real account usage:

```sh
python3 -I -B scripts/check-codex-browser.py \
  --live-account \
  --urdia ./target/release/urdia \
  --codex "$(command -v codex)" \
  --model YOUR_ACCESSIBLE_MODEL --effort low \
  --chromium "$(command -v google-chrome)" \
  --output "$HOME/urdia-codex-browser-evidence"
```

The check sends two fixed calculator tasks. In the first it rejects the edit and
checks that no file was created. In the second it confirms at most two expected
edit requests through the actual browser buttons, rejecting execution and all
other permission kinds. It verifies the exact resulting file set and independently
runs the three generated unittest cases. No trusted session is activated.
The temporary workspace/database/configuration are removed on exit; only
sanitized screenshots and a summary remain in the requested output directory.
No API keys, private URL tokens, raw transcript exports or browser traces are saved.
Native Codex's own logging/retention is still outside this check's control.

The browser checks streaming, reported model/effort, completed edit telemetry,
frozen host durations, redacted one-shot receipts, history and mobile overflow.
The first denied turn may finish normally: a completed conversation turn does
not prove the requested project change succeeded. Independent file/test checks
are the evidence for the second task.

On 2026-10-02 this passed against native Codex 0.159.2 and the existing account:
one rejected edit, one allowed edit creating both requested files, three host-run
tests, one completed reported edit, 17.5 seconds total host duration and 3.4 seconds
until first text on the allowed turn. These are observations of one run, including
permission waiting, not a model benchmark or a price/token measurement. The
actual Windows/WSL Codex dashboard check remains pending on the owner's PC.

## Reviewed planning samples

The [planning evaluation](planning-evaluation.md) records six explicitly authorized
native Codex samples through Urdia: five host-valid outlines and one incomplete
response rejected without plan adoption. It separates schema checks from manual
quality review, documents an opt-in bounded runner and records unreported cost and
token data as unknown. This is not evidence for autonomous planning or routing.

## Stream integrity

The bridge reconciles each streamed message with Codex's authoritative final
message, forwarding a missing suffix once. A conflicting or missing final state
fails the turn instead of claiming complete output. Text and item counts remain
bounded; overlapping message streams cannot be reordered and fail closed.
Malformed planning JSON is still rejected by the host: no invented closing
characters or automatic model retries. This follows the
[App Server message lifecycle](https://learn.chatgpt.com/docs/app-server).

## Safe failure explanations

Recognized structured native error information is converted into a fixed failure
category for the host. Native messages and additional details stay private; missing
or unknown information stays generic. Scoped failed-turn handling and bridge shutdown
remain unchanged. There is no automatic reconnect, retry or model fallback. After
correcting an external issue, use the explicit browser reconnection control in a
non-trusted host, or choose a loaded fresh profile. Native conversation context
starts fresh; host plans and previous task evidence remain. Trusted hosts require
an explicit restart and renewed trust.
See [agent failure recovery](provider-failures.md).
