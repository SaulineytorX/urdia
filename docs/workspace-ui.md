# Visual workspace

`urdia chat` opens a private, loopback-only workspace with four views:

- **Conversation**: the selected project and agent, streaming turns, a next-task
  composer, cancellation, current task status, and opt-in permission controls.
- **Activity history**: redacted persistent events, separated from the current
  conversation. “Session not closed” does not mean a task failed. “Session
  closed” describes the lifecycle, not whether every earlier task succeeded.
- **Decision engines**: connect and test an installed local Laya service directly
  in the browser. `--advisory-ui` additionally enables host-configured engines.
  Recommendations never switch or start an agent.
- **Setup guide**: configure agents/profiles and review an explicit test task.

Conversation identity comes from the active host session, not the newest entry
in the database. The authenticated conversation response exposes the selected
provider ID/name and project directory name, without its parent path. This
identity is absent from the public shell and diagnostic Debug output. As with
conversation text, treat the private URL and authenticated response as private.

Only one turn executes at a time. You can draft the next message while it runs;
cancelling preserves that draft. Polling does not replace unchanged transcript
nodes. New output follows the bottom only when you are already near it.
Prompt and response text is displayed literally, without executing HTML or
rendering Markdown. Content retention remains off unless explicitly enabled.

Permission behavior is unchanged: one-shot rejection or separately confirmed
allow-once. A trusted session requires the existing explicit CLI opt-in,
prepare/confirm flow, exact host scope, expiry and revocation. A routing model
cannot grant either form of access.

## Choosing decision intelligence

Jev, Laya, and TypeLLM are separate from the coding agent and its model. A Qwen
session can use a hosted coding model even when a local decision service exists.
Use the [provider guide](providers.md) for each implemented adapter's configuration.

For Laya, open **Decision engines → Laya**, enter the numeric loopback address
(default `http://127.0.0.1:18766`) and the local service key, then click
**Connect and test**. No environment variables, terminal configuration or restart
are required. On success Laya becomes selectable for recommendations. **Test
current connection** reuses its in-memory key; **Disconnect** removes that
connection from the session. A failed replacement keeps the previous connection.

The connection test sends one small, fixed English decision request to the local
service, checking authentication and the typed protocol without sending project
files or the user's task. It does not certify recommendation quality. The status
records the last successful test; polling does not continuously test the service.
If the service is still warming up, a timed-out test can be retried once ready.
Only HTTP(S) numeric IP loopback addresses are accepted; proxies and redirects
are disabled by the adapter. Invalid requests and errors never echo the key.

Urdia clears the password field on submission and does not write this key to
browser storage, configuration files, SQLite or the agent environment. The
connection belongs to the current host process and survives a page refresh,
but is forgotten when Urdia stops. Memory release is not a secure-erasure
promise. Persistent Laya credential storage remains future work.

Laya must already be installed and running **on the host running Urdia**.
A service installed on another server is not automatically available on your PC.
This form connects a service; it does not install software, download a checkpoint
or modify system services. A supported installation guide is in
[installation](installation.md#optional-local-decision-services).
Jev now offers [browser configuration and optional OS credential storage](jev-credentials.md).
TypeLLM still uses host configuration with `--advisory-ui`. Keys are never returned
to the browser. Configured startup adapters are not tested
until an explicit request.

For optional legacy host-environment setup, from a source checkout in
`~/projects/urdia`, first enter the key privately in a
Linux/WSL terminal. Run these two lines, type/paste the key when prompted, then
press Enter before running the next block:

```sh
read -rsp 'Jev API key: ' TYPESAFE_API_KEY
printf '\n'
```

Start the host using the complete binary path (the built binary is not necessarily
on PATH and your project directory is not the source checkout):

```sh
export TYPESAFE_API_KEY
"$HOME/projects/urdia/target/release/urdia" chat \
  --provider qwen \
  --workspace "$HOME/projects/urdia-preview-project" \
  --permissions-ui --advisory-ui
unset TYPESAFE_API_KEY
```

`chat` stays in the foreground until stopped. Unsetting a parent terminal's
variable does not remove the child's inherited copy. Stop the host to release
its configured adapters. These instructions do not write the key to a config file,
browser storage or shell history. Agents still inherit host environment in the
current MVP; Urdia does not provide secret isolation against same-user agents.
Use only trusted projects and agents on this host.

Open **Decision engines**, select Jev/Laya/TypeLLM or offline rules, enter the
objective and explicitly request a recommendation. Jev requires checking the
remote quota/data consent box for each request. Only the objective and registered
provider descriptions are sent by this feature; project files are not read.
Opening the view or polling status performs no model call. Rules choose the
first registered candidate without AI and are labeled accordingly.

A valid response can recommend an agent or decline to select one and ask for
review. Provider errors are a separate fallback. Probabilities and confidence
are displayed with their reported semantics, not as success rates. Recommendations
remain separate from the conversation; they do not execute the objective.

Completed and failed recommendation requests show **Host decision time** in
milliseconds. This monotonic measurement covers admission through the terminal
outcome, including connection, adapter retries and host scheduling; it is not
GPU inference time or coding-agent execution time. It freezes on completion and
matching retries reuse it without another request. Running requests have no final
measurement. These session-only observations do not include tokens, costs or
estimated savings. The result also makes explicit that task verification has not
been performed: a model's confidence, an agent's completion report or its claim
that tests passed is not independent evidence. Permissions remain a separate
host check.

Authentication protects GET/POST `/api/advisory` and POST `/api/advisory/laya`. Each accepted request has a
UUID: matching retries return the existing job, changed payloads conflict. One
request runs at a time, with a 35-second deadline and 64 accepted requests per
host process. Completed or failed requests are not repeated by browser retries;
existing adapters can perform their own bounded upstream retries. Requests and
results stay in host memory and disappear on restart. Laya connection changes
share the active slot with recommendations, have a 35-second deadline and at
most 16 connection/check attempts per process. Disconnect remains possible
after that limit. Laya persistence and broader credential management, in-session profile switching and automatic
routing remain future work; [ADR 0015](adr/0015-automatic-routing-activation-gate.md) still applies.
See [ADR 0017](adr/0017-explicit-browser-advisory-routing.md) for the API boundary.

## Optional browser acceptance

Build the workspace and run `scripts/check-workspace-browser.py` with explicit
paths to Urdia, the bundled fake agent, a local Chromium executable, and a private
output directory. The script requires Python Playwright (verified with 1.63.0),
installed in an isolated virtual environment. It is optional and not a new
runtime or hosted CI dependency. On a Linux host with Chrome already installed:

```sh
cargo build --workspace --locked
python scripts/check-workspace-browser.py \
  --urdia target/debug/urdia \
  --fake-agent target/debug/urdia-fake-agent \
  --chromium /usr/bin/google-chrome \
  --output /absolute/private/browser-evidence
```

Run `python` from the environment containing Playwright. Tests use disposable
credential-free host/agent processes on ephemeral loopback ports. They cover
navigation, two turns, inert HTML, stable transcript nodes, responsive widths,
authentication lock, cancellation with a draft, one-shot reject/confirm, and
trusted prepare/confirm/revoke, and a real offline advisory recommendation without
agent execution. Remote consent/review/error, failure and history-only screens use controlled
HTTP fixtures, not a real provider error. Screenshots contain only synthetic
tasks, without access URLs or browser traces. The script stops its processes and
removes its temporary workspaces.

## Current execution profile

Sessions started with `--profile` or `default_profile` show an authenticated
profile summary: configured agent, inference provider, model, optional effort
and whether the default or an explicit choice was used. These are declared
configuration labels and visibly unverified, not live model detection. Labels
use literal DOM text, and the summary is absent for direct `--provider` sessions.
Nothing is inferred from older timeline entries. Commands, argv, credentials
and full workspace paths are omitted. The profile remains fixed for this session.
See [execution profiles](execution-profiles.md) for CLI/WSL launcher commands.

## Experimental profile advice

Add `--advisory-profiles` to `chat --advisory-ui` (or the source launcher) to
recommend configured profiles rather than bare agents. The engine view identifies
this mode, displays the recommended profile's declared model/effort and keeps
its unverified status explicit. Both selected-profile text and probability labels
are inert DOM text. This request never submits a conversation turn or changes
the active/default profile. Existing consent/idempotency/deadline rules apply.
See [profile configuration](execution-profiles.md#experimental-profile-recommendations)
and [ADR 0019](adr/0019-experimental-profile-advice.md).

### Register an installed agent in the dashboard

Start the host once with explicit private launch and profile settings files:

```sh
mkdir -p -m 700 "$HOME/.config/urdia"
"$HOME/projects/urdia/target/release/urdia" \
  --agent-store "$HOME/.config/urdia/agents.toml" \
  --profile-store "$HOME/.config/urdia/profiles.toml" chat \
  --provider qwen --workspace "$HOME/projects/urdia-preview-project" --permissions-ui
```

This requires an installed, logged-in Qwen to start the initial conversation.
The source launcher also accepts both store flags. Existing external launches
can still use `--config`; keep it on subsequent launches. All three files must
be separate. Their parents must exist and be private enough for the store checks;
do not create empty files first. Registration does not install an agent.

In **Decision engines → Your coding agents**, open **Register an installed ACP
agent**. Choose the Qwen starting point for another dedicated Qwen launch, or
enter a trusted ACP executable, a new ID and its argument array. For example,
Qwen uses `qwen` and `["--acp"]`. Arguments are a JSON array, not a shell command;
`$HOME`, `~` and shell substitutions are not expanded. Never put API keys or
passwords in the executable, arguments or display name. The agent manages login.
Confirm that you trust this launch and save. Saving starts no agent.

The new entry shows **Pending restart**, and appears immediately in the profile
editor as **setup only**. Create its profile and optionally inspect its reported
models/efforts in the same session, then save. Stop and restart Urdia **once** with
the same store/config flags, using `--profile ID` or omitting `--provider` for your
saved default. Both agent and profile now apply together. Running tasks and
recommendations keep their original settings throughout setup; pending launches
cannot receive tasks from the current host.

For Codex, use the optional [native bridge](codex.md): the executable is your
absolute Python path, and the array is `["-I", "-B", "/absolute/path/to/urdia/scripts/codex-acp.py", "--codex", "/absolute/path/to/codex"]`.
Its pinned version and native startup checks still apply. Claude or another CLI
needs an installed compatible ACP adapter; entering its ordinary chat command is
not sufficient. The form cannot verify login or ACP compatibility.

Executable and arguments are saved privately on the host and omitted from
catalog responses. Successful registration clears their form inputs. Registration
currently adds new IDs; it cannot edit or delete active launches. Duplicate IDs
across the built-in provider, config and store fail on startup. After external
edits or a save conflict, review the store and restart rather than overwriting it.
See [ADR 0026](adr/0026-dashboard-agent-registration.md).

### Create and edit profiles in the dashboard

Decision engines now shows an **Execution profiles** catalog with available
registered launches, declared model/effort and exact pins. To enable its editor,
choose a private settings file once when starting the host:

```sh
mkdir -p -m 700 "$HOME/.config/urdia"
"$HOME/projects/urdia/target/release/urdia" \
  --profile-store "$HOME/.config/urdia/profiles.toml" chat \
  --provider qwen --workspace "$HOME/projects/urdia-preview-project" --permissions-ui
```

For external agents, include the same `--config /path/to/providers.toml` used to
register their launches. This example requires an installed, logged-in Qwen;
it does not install an agent. The store's parent must already exist and on Unix
must not be writable by group/others. Do not create an empty store first; an
absent file is created privately at the first successful save.

Open **Create or edit a profile**, select an active or saved pending launch, enter
its name/model/effort and the exact supported selector IDs, and choose whether
it should become your default. Save, stop the host, and restart with the same
`--profile-store` and provider-config flags. Omit `--provider` to use the saved
default, or select `--profile ID`. Add `--advisory-ui --advisory-profiles` once
profiles exist to recommend among them. Saving itself makes no AI request.

The store contains the full profile catalog/default and overrides profile entries
from the provider configuration on the next launch. Registered commands remain
in the original provider configuration; the editor cannot add arbitrary commands
or save API keys. Each profile needs a dedicated launch. Model labels alone do
not configure an agent or establish its capabilities; exact pins are checked
before a confirmed task receives its prompt.

Current tasks, default conversation and recommendation tickets do not change
when you save. Pending-restart status makes that explicit. Unsaved drafts survive
polling; concurrent or external edits are rejected when detected. After an
external file change, review it and restart the host before saving again. Use one
writer per store; filesystem checks do not isolate other processes running as you.
See [ADR 0025](adr/0025-dashboard-profile-settings.md).

### Discover reported model and effort options

In the profile editor, select an active or saved pending launch and open **Discover
model and effort options**. Confirm the temporary startup, then click **Inspect
available options**. Urdia starts the trusted agent in this workspace, sends no
task prompt, denies permission requests and closes the temporary session after
reading its bounded ACP selectors. Agents may contact their own service or keep
native records during startup; only inspect launches you already trust.

Choose a reported model to copy its exact ID into the draft. This clears previous
effort settings and disables old effort choices. Click **Inspect available options**
again to select that model in a fresh temporary session and obtain its effort
choices, then choose an effort. Labels and pins remain editable. Inspection never
saves a profile or changes your default/running task. Save through the private
profile store and restart to apply the draft as described above.

There are at most eight inspections per host, one at a time, with a 30-second
limit. Repeated delivery of the same request key/selection reuses its result;
a changed selection uses a new key. If delivery is uncertain, retry the same
selection. Restart after reaching the limit. Private failure details stay hidden;
check the agent's ACP compatibility and login in the same Linux/WSL environment.
A saved pending registration can be inspected during setup; this does not activate
it for tasks. An agent that does not report selectors can still be configured manually.

Choices are agent reports, not account-access or inference guarantees. Native
Codex currently reports only its confirmed current selectors; this is not a full
account model catalog. Exact model/effort settings are checked again before a
confirmed task receives its prompt. No Jev/Laya call is part of inspection.
See [ADR 0027](adr/0027-explicit-control-discovery.md).

### Review and run a recommended profile

In profile-advice mode, a successful recommendation with an available dedicated
launch and exact `session_controls.model` (plus `session_controls.effort` when an
effort label exists) offers **Review and run this task**. Label-only profiles,
bare-agent advice and review/error fallbacks cannot launch a task this way.

1. Request advice from Rules, Jev, Laya or TypeLLM. Rules is an offline test;
   configured engines are not evidence of comparative quality.
2. Review the original objective, project, profile and exact selectors. Editing
   the advice draft afterward does not change this reviewed task.
3. Select **Confirm and run** within five minutes. This starts a fresh session
   for the selected profile and can consume its provider quota. A newer
   recommendation invalidates an unaccepted review.
4. Follow Conversation for the task's actual agent identity, reported settings,
   duration, streaming and tool activity. Allow/reject operation requests separately.
   Verify actual files/tests; a model's answer alone is not proof they passed.

Safe retries reuse the same ticket/turn and do not start duplicate work. A busy
conversation can be retried when the current task ends. Startup/cancellation or
pin mismatch never silently selects another provider. The primary agent/default
is unchanged for your next ordinary message. Your configured profiles are loaded
once; restart after changing the configuration file.

The feature is unavailable in a host started with `--trusted-session`. Use the
normal one-shot permission UI for this initial flow. Automatic routing remains
disabled under ADR 0015. See [ADR 0024](adr/0024-confirmed-profile-task-execution.md).

### Agent-reported startup controls

Current authenticated conversation context may include `startup_controls` with
only initial model/effort value IDs. The workspace labels it as a startup report,
separate from the operator's declared profile. Remote inference is unverified;
later agent configuration notifications are shown separately in the latest
report. The startup snapshot itself remains immutable.
Raw selector names/descriptions/metadata are omitted, and the context remains
memory-only and absent from debug output. No browser controls can change it.

## Host-observed task metrics

The metrics cards above the conversation display total elapsed time from acceptance, execution time
from the host marking the turn running, latency to the first nonempty text update
from that point, text-update count and Unicode response-character count. All
intervals use the host monotonic clock. They include provider waits, permission
waits, host processing and final event writes; they are not inference benchmarks.
Queued tasks have no execution/first-text measurement. Non-streaming agents may
never report text updates. Missing observations display `Not observed`.

Measurements stop when the host marks a turn completed, failed or cancelled.
Cancellation requests alone do not stop the clock. Prompt retries reuse the same
turn; empty or rejected late text updates do not increase activity counts. Only
metrics for the bounded in-memory history are retained (64 turns), through the
same authenticated API as the conversation. They are not saved to SQLite or
restored after restart. API snapshots recompute active durations even when the
conversation sequence is unchanged; the browser updates the metrics cards without
rebuilding an unchanged transcript.

The activity line distinguishes waiting for text, text already received, a
pending permission decision and a terminal turn. Tool notifications are displayed separately as agent reports, not independently
verified execution. Tokens and cost remain `not reported`; characters and update
counts are not tokens. Task completion is the end of a turn, not independently
verified code correctness. No additional AI calls are made by these metrics.

## Redacted tool activity

The task rail now shows ACP `tool_call` / `tool_call_update` reports: a turn-local
ordinal, a fixed category and the last reported status. It handles partial updates,
including an `unknown` status when no status has yet been reported. Repeated reports
are deduplicated, statuses do not move backward and the first terminal status is
retained. Permission requests alone do not create execution reports or grant access.

Only the active ACP session may publish text/tool updates, and tools are accepted
by the conversation hub only while the host turn is running. After cancellation
or completion, existing states are retained as last reports; pending tools are
not falsely marked successful or failed. ACP notifications have no prompt-turn
identifier, so delayed same-session notifications arriving during a later turn
cannot always be attributed to their original turn. These are observations, not
execution receipts, verified test results or a sandbox.

The tracker keeps at most 128 tools per active turn, with bounded printable ASCII
IDs used only internally. Browser/API data contains no original IDs, names, titles,
paths, arguments, results, diffs, terminal output or metadata. Category/status are
fixed host enums, rendered through literal DOM text. The interface shows the latest
eight entries and a summary of the retained set; an explicit warning identifies
truncation. Existing retained tools can still receive updates after the cap is
reached. Memory-only reports are pruned with the 64-turn conversation history;
no new SQLite fields or persisted telemetry are added.

Agents that do not report tools retain the normal chat flow, without invented
activity. No AI call, profile selection, permission or trust change is triggered.
See the [ACP v1 tool-call contract](https://agentclientprotocol.com/protocol/v1/tool-calls).

## Guided workspace setup

Open **Setup guide** in the sidebar. The five steps use the same forms as
Decision engines, preserving drafts when you move between the two views.
Navigation alone never starts an agent inspection, decision call or coding task.

1. **Project:** confirm the current project and active agent. Start Urdia with
   another workspace to use a different folder; this page cannot switch folders.
2. **Agents:** register an installed, trusted ACP launch. Login and installation
   remain the agent's responsibility. Read-only catalogs require restarting with
   the private agent-store flag before registrations can be saved.
3. **Models & effort:** explicitly inspect reported selectors, choose the model,
   inspect its effort options and save the profile/default. Private profile-store
   flags are required for saves. Pending registrations can be configured now.
4. **Decision engine:** choose manual operation or configure the already installed
   local Laya service. Jev uses the existing host-key instructions, not browser key
   entry. Advice and coding execution have separate confirmations.
5. **Try it:** confirm a small arithmetic prompt to the currently active agent.
   This can consume your account quota. The result shows the response and measured
   time. Only a completed response of exactly `20` passes this smoke check; this
   does not certify coding quality or verify the remote model.

Finish all pending settings, then restart once with the same stores before the
final test. A busy host, active trusted access or unconfirmed live configuration
also blocks it. Use **Follow task / permissions** to see the execution, cancel it,
or decide an unexpected permission request. The prompt requests no tools or
files, but Urdia is not a sandbox. This test never enables trusted access.

An uncertain delivery can be retried with the same test ID. A recorded test cannot
be submitted twice from the same page. Guide progress and its test result are
page-memory state; reloading opens a fresh guide and does not replay the test.
The previous turn remains in the current conversation/history.

## Portal dashboard layout

The conversation uses the Portal identity: a branded start screen, separate user
and agent message surfaces, and a five-card metrics strip above the transcript.
Metrics remain hidden until a task has observations and use only the existing
host-measured fields. Token and cost information remains explicitly unavailable.
On narrow screens the strip wraps into fewer columns without hiding values.

The side rail keeps current-task status, redacted tool reports and permission
controls. Running tasks use cyan, permission waits/stopping use amber, completed
turns use mint and failures/cancellations use coral. Text labels always accompany
the colors. Completed turns still need independent verification of files/tests;
reported tool activity is not proof of execution. This layout adds no agent calls,
permissions, charts, progress percentages or routing behavior.

## Switch loaded profiles without restarting

In **Conversation**, use **Agent for your next tasks**, inspect the preview's
agent/model/effort selectors, then press **Apply selection**. The applied choice
is displayed separately from an unsaved preview. Selection starts no agent and
uses no account quota; sending a task starts the selected agent and can consume
quota. Its exact requested selectors must be confirmed before the prompt is sent.

Each selected-profile task gets a fresh session in the same project, its own
session history and individual permission requests. Earlier chat text is not
forwarded to another agent; project files remain available in the fixed workspace.
The selection remains for following tasks until changed. Choosing **Original host
session** restores ordinary prompts to the original ACP session and its context.

The selector is disabled during a task and while delivery is uncertain. The server
also rejects active-turn switches and stale revisions from another tab. On a stale
selection, review the applied choice before retrying. An accepted task retry keeps
its original target even after the default changes. A selection delivery failure
never starts an agent automatically. If the selected profile changes during an
uncertain setup-guide test delivery, reload the guide after reviewing the applied
choice to start a new explicit test; retries never silently retarget it.

Only loaded, available profiles with exact controls appear. This works without
`--advisory-ui` or `--advisory-profiles`, but requires execution profiles to exist
in the host configuration/stores. Saved new agents/profile definitions still need
one restart. This host-memory selection does not alter the saved default and is
reset at host restart. Hosts launched with `--trusted-session` do not expose it.
Use an ordinary one-shot/deny-by-default host for this experimental flow.

The authenticated API adds `POST /api/conversation/profile` with
`{revision, profile_id, confirm:true}`. Ordinary prompt requests must include the
current `profile_revision` when the conversation snapshot exposes
`profile_selection`; omit it for hosts without this feature. Advice confirmations
remain independent, explicitly reviewed one-off overrides. Automatic routing is
still disabled. See [ADR 0030](adr/0030-manual-next-task-profile-selection.md).

## Independent calculator checks

Start chat with `--verification-ui` (Linux/WSL and Python required). After the
latest task completes, the Conversation view offers **Verify this calculator**.
Expand the review, inspect the two files, authorize their Python execution and
run the fixed checks. The response from your coding agent remains separate.

See [verification](verification.md) for the exact contract, deadlines and privacy
boundary. The result records check-time observations, becomes historical after
a new task and never claims general project verification. Opening the panel or
polling starts no command or model call. Matching retries do not rerun accepted
checks. New tasks/profile switches wait for check completion or cancellation.

## Reviewed project plans

Conversation includes a host-session **Project plan** for 1–8 operator-reviewed
steps. Review scope and acceptance, prepare a draft, explicitly send it to the
selected agent, inspect the result and mark your review before the next draft.
It starts no task on its own. Reviewed state survives browser refresh; restarting
Urdia clears it. **Save plan file** and **Recover a saved plan** provide explicit
portable recovery, with fresh task/review state and separate historical notes.
See [project plans](project-plans.md)
for task binding, retries, profile selection and limits. With profile advice enabled,
**Find a profile for this step** prepares explicit Jev/Laya advice and a stage-bound
execution review. The host checks freshness again at confirmation.

Project plan also offers an objective-first agent proposal: prepare a message,
explicitly send it, review its completed response, load it into the editable outline
and accept it. No JSON copying or additional key configuration is required beyond
your selected agent. Shape validation is separate from quality and permissions.

## Failed task explanations

The live task panel and transcript show the same fixed host explanation for a known
agent failure. History retains only an allowlisted category. An unknown cause stays
unknown; raw provider errors are not rendered. No error triggers an automatic retry,
profile/model switch or permission. See [failure recovery](provider-failures.md).


## Reconnect a failed project agent

After an original-agent failure, a non-trusted workspace shows **Recover your
agent** in the task panel. Correct the external problem, check **Start a new agent
conversation**, then select **Reconnect agent**. Nothing is resent automatically.
Your project plan, task history and next draft remain; the new agent receives no
earlier transcript. Prepare/send the next task explicitly. A fresh-profile failure
does not replace the original agent. Pending work, verification or permissions
blocks replacement, and trusted hosts require an explicit restart instead.

A failed reconnect leaves the host UI running with a fixed diagnostic and renewed
consent for another attempt. See [recovery details](provider-failures.md), including
startup bounds, environment changes and the distinction from account verification.


## Project results at a glance

A reviewed plan shows a read-only results panel in Conversation. Each step keeps
agent completion, your review and explicit task checks separate. It includes the
latest attempt time and a **View task** link while that transcript is retained.
Earlier checks are labeled historical when later tasks may have changed files; a
retry never inherits another attempt’s pass. No task starts from this summary.
See [the summary guide](project-summary.md) for measurement and evidence limits.


## Check a Python project beyond calculators

Open **Check your project**, choose **My Python project · existing unittest tests**,
review the scope and explicitly authorize **Run selected checks**. The helper runs
existing stdlib unittest tests; it installs nothing and makes no AI call. Empty,
failed or skipped suites cannot pass. Check results remain separate from operator
review. See [project checks](python-project-checks.md) for source limits and
[first preview scope](first-release.md) for release readiness.

### Static frontend checks

The project check selector now offers [static frontend and web-counter checks](frontend-checks.md).
Review execution consent before running. Optional sandboxed browser tooling must
be configured separately; no dependencies or model calls are started by the panel.

### Optional automatic Jev preview

[Automatic Jev](automatic-jev.md) is selected in the task composer. It dispatches
one visible submitted task through the existing host ticket/execution path, with
exact model/effort application and no tool permission changes. Manual is default.

## Your first checked task

Open **Setup guide** to see the current project, agent response, configured
profile and latest output-check status. These are host observations, not an
installation or remote-model certificate. Registered agents have not necessarily
responded successfully; a completed turn is separate from passing tests. Earlier
checks cannot certify a newer task. Each passing result identifies its check scope.

For a small first exercise, start Urdia with an **empty practice project** and a
real coding agent. The guide does not inspect whether the folder is empty.
Review the two-file Python addition task, confirm you are using that practice
folder, and choose **Prepare first task**. This fills an editable draft in
Conversation and selects the matching **Two-number addition** check contract.
It preserves an existing draft, clears check consent, and changes neither your
agent nor your routing mode. Review those choices and explicitly send the task.

After the agent finishes, inspect calculator.py and test_calculator.py. Open
**Check your project → Review checks and execution permission**, review the
selected checks and authorize their execution. They run as your OS user and are
not sandboxed. The guide then distinguishes agent completion from the current
host check result; it does not claim every requirement or project is verified.

The offline echo demo cannot create this exercise's files. Missing/disconnected
hosts, pending saved settings, busy work, unresolved deliveries, pending
permissions and active trusted access block this preparation. The existing five
setup steps and separately consented arithmetic response test remain available.
No credential collection, installation, task dispatch or checks happen merely
by opening the guide or preparing a draft.

## Open a web result

After a completed task, use **Open your web result** in Conversation to prepare
and open a trusted static project in a separate tab. Linux/WSL only; requires
`index.html` at the workspace root. Nothing is installed or built. Review the
[preview guide](result-preview.md) for consent, temporary storage and limits.
