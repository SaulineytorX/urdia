# Execution profiles

An execution profile gives a configured ACP launch a stable name and records its
agent, inference provider, model and optional effort. It can be selected for
`run` or `chat`, or set as the default in an explicitly supplied TOML file.
This is the first building block for using different profiles for backend,
frontend and verification tasks. Task decomposition, live switching and
automatic profile dispatch are not implemented by this slice.

## Configure a profile

Add this structure to your provider configuration. These are placeholders, not
verified flags for Claude, Codex or Qwen:

```toml
default_profile = "backend"

[[provider]]
id = "backend-launch"
name = "Backend agent"
command = "/absolute/path/to/your-acp-agent"
args = ["--your-tested-acp-option"]

[[profile]]
id = "backend"
name = "My backend profile"
provider = "backend-launch"
agent = "Your coding agent"
inference_provider = "Your inference service"
model = "Your configured model"
# Omit effort if your agent does not support an effort control.
# effort = "Your configured effort"
```

The referenced provider's exact command and argument array perform execution.
Profile labels **do not set or verify** model/effort. Configure those through
that agent's documented options or configuration and verify them independently.
Urdia does not guess CLI flags, normalize effort names between providers, or
change the agent's authentication. Each profile must reference its own registered
provider definition; profiles cannot give conflicting labels to the same launch.
For two models of the same agent, register two launch definitions that actually
select their corresponding configurations.

There are at most 64 profiles. Profile IDs use lowercase kebab-case, must be
unique, and the default must name an existing profile. Names, agent, inference
provider, model and optional effort are nonempty labels of at most 128 UTF-8 bytes
with no control characters. Unknown TOML fields and unknown provider references
fail before launching an agent. Discovery does not inspect credentials.
Do not put credentials in labels, commands or arguments.

## Dashboard editor

Use the authenticated [profile editor](workspace-ui.md#create-and-edit-profiles-in-the-dashboard)
with an explicit `--profile-store PATH` to create/edit profiles for already
registered launches, without hand-editing TOML. The profile-only file becomes
the full authoritative catalog/default at the next start with that flag;
launch commands still come from `--config`. Saving cannot change running sessions
or advice and makes no inference request. Restart with the same flags to apply it.

Without `--profile-store`, the dashboard catalog is read-only. Model IDs are
entered explicitly; supported IDs are not guessed or automatically enumerated.
The source preview launcher accepts the same flag for preflight and launch.

## Inspect and select

From a source build inside Linux/WSL:

```sh
./target/release/urdia --config /path/to/providers.toml profiles --json
./target/release/urdia --config /path/to/providers.toml chat \
  --profile backend --workspace /path/to/project --permissions-ui
./target/release/urdia --config /path/to/providers.toml run \
  --workspace /path/to/project --prompt 'Review this project'
```

The last command uses `default_profile`. `--profile` overrides that default;
`--provider` bypasses it and selects the registered agent directly. Providing
both flags is rejected. Without either flag or a configured default, session
commands fail before creating a database. Unavailable executables still fail
through the existing provider resolution checks.

`profiles --json` lists profiles in ID order, reports which is the default, and
reports executable availability separately from `model_effort_verified: false`.
It omits launch commands and arguments. Executable availability does not prove
ACP compatibility, authentication, model identity or effort support. Existing `providers`, CLI `route` operates on registered provider IDs. Browser advice compares available
configured profiles when present, otherwise registered providers. Explicit experimental profile advice is available
with the additional opt-in below.

The source preview launcher also accepts `--config` with `--profile`; without
an explicit provider/profile it uses the configured default if present, otherwise
its existing Qwen fallback. `--provider` bypasses the profile default. `--demo`
cannot be combined with profile configuration. `--check` validates availability
without starting an agent or writing session state.

```sh
python3 -I -B scripts/start-project.py \
  --workspace "$HOME/projects/your-test-project" \
  --config /path/to/providers.toml --profile backend
```

The conversation shows the current profile's declared agent, inference provider,
model and effort, distinguishes default from explicit selection, and marks model
and effort unverified. These fields come from authenticated host context, not
historical events. No commands, arguments or keys are sent to the browser.
The profile stays fixed for the session. Browser selection/switching and routing
over verified profiles are subsequent slices. Permissions, authenticated loopback access,
cancellation and content retention retain their existing behavior. A profile
never grants trust or permissions. Agent children still inherit host environment;
this feature introduces no credential isolation or sandbox.

## Experimental profile recommendations

With configured profiles, opt in explicitly:

```sh
python3 -I -B scripts/start-project.py \
  --workspace "$HOME/projects/your-test-project" \
  --config /path/to/providers.toml --profile backend \
  --advisory-ui --advisory-profiles
```

Open Decision engines and submit an objective. Rules are an offline transport
check, not AI. Jev/Laya/TypeLLM use existing host configuration; Jev additionally
requires per-request remote consent and can consume quota. Configuration is not
connection or quality verification. Profile labels are sent as declared metadata;
do not put secrets in them. Commands, argv and project files are not sent.

Results show a profile and its declared model/effort, or an explicit review/error.
Unavailable executables are excluded; an empty profile set fails startup before
session state is written. A model or effort label does not establish support,
quality, speed or cost. The request does not start a task, change the active
profile or override your default. Ordinary `--advisory-ui` without the additional
flag retains provider-level advice. A separate **Review and run** confirmation can
start one task for an eligible profile with exact ACP model/effort pins, in a new
session with independent one-shot operation permissions. It does not change the
primary/default agent and is unavailable with `--trusted-session`. See
[dashboard execution](workspace-ui.md#review-and-run-a-recommended-profile) and [ADR 0019](adr/0019-experimental-profile-advice.md).

## Checking Qwen controls without inference

See [Qwen control evidence](qwen-controls.md) for the disposable ACP probe of
version 0.24.7. It confirms agent-reported model/effort selection and refresh,
not effective remote inference. Profile declarations remain unverified; the
running host does not yet apply those ACP selectors automatically.

## Optional native ACP startup controls

Labels (`model`, `effort`) remain display-only. A profile can separately request
exact opaque values advertised by its agent's ACP v1 selectors:

```toml
[[profile]]
id = "qwen-medium"
name = "Configured Qwen medium"
provider = "dedicated-qwen-launch"
agent = "Qwen Code"
inference_provider = "Your configured service"
model = "Qwen 3.8 Max"
effort = "Medium"
session_controls = { model = "qwen3.8-max(openai)", effort = "medium" }
```

Register the dedicated provider separately as described above. These example
values were observed with Qwen 0.24.7 under a fixture; your own configured agent
must advertise them. Do not guess model IDs or use a generic effort ladder.
Either requested field may be omitted. Unknown fields and invalid identifiers
are rejected without echoing their values. IDs are nonempty printable ASCII,
maximum 1024 bytes. Profiles remain operator-controlled launch definitions.

Both `run` and `chat`, including the project launcher, apply controls from an
explicit/default profile. Direct `--provider` selection bypasses profile controls.
Readiness checks and `profiles --json` do not start an agent or validate its
options. Startup validates the agent's advertised selectors, changes model first,
refreshes the complete option list, then validates/changes effort. Each requested
selection must be confirmed, including the model after setting effort. Unsupported
values, ambiguous/malformed selectors, RPC failures and mismatched confirmation
stop startup before a prompt. The existing overall startup deadline applies;
there are at most two configuration changes and no retries.

This first integration supports unambiguous `model` and `thought_level` select
categories with flat/grouped values. It omits mode, permission, boolean and custom
controls; missing categories are not guessed from labels. Limits are 64 selectors
and 128 values/groups per relevant selector. Agents without `configOptions`
continue working when no controls are requested, but cannot satisfy an explicit
request. No live browser switching is implemented.

The authenticated workspace displays **agent-reported startup settings** separately
from declared profile labels. Only model/effort IDs are included; names,
descriptions, metadata and selector IDs are omitted. This snapshot stays in
memory, outside debug output and event persistence. It is not a live guarantee:
an agent may later change its configuration. Remote inference, model quality,
cost and effective reasoning remain unverified (`model_effort_verified: false`).
Advice cannot apply these settings or change the active/default profile.
The profile-advice fingerprint still does not bind all configuration/evidence.

The integration follows the [ACP v1 session configuration contract](https://agentclientprotocol.com/protocol/v1/session-config-options).
See [Qwen evidence](qwen-controls.md) for the separate offline and native-host checks.

## Selection evaluation checkpoint

The frozen [profile evaluation](profile-evaluation.md) tests synthetic task fitness,
user pins/defaults and host compatibility/freshness rules. An initial real Jev
development pass accepted eight of eight cases. It does not verify your own
profiles or activate dispatch. The reserved result is recorded below; Laya/TypeLLM
v5 comparisons remain pending. Full-context report bindings are separate from the existing
browser candidate fingerprint and do not change browser advice.

The subsequent reserved Jev pass accepted 7/8 in each partition and selected one
unsupported requested model. The [failure analysis](../evaluations/routing/jev-1.13.0-v5-profiles-reserved-1x-2026-10-02.md)
confirms that semantic advice cannot replace host compatibility checks. Automatic
execution remains disabled; explicit native ACP launches still validate advertised
controls and reject unsupported requests before task submission.

## Live reports and configuration drift

The authenticated conversation now includes `live_controls`: the latest bounded
agent report, a monotonic notification revision, whether it differs from startup,
and whether explicitly requested controls still match. ACP `config_option_update`
replaces the complete set; values removed by an update are cleared. Malformed or
ambiguous decoded selectors produce an unavailable report, without exposing raw
payloads. Reports for another ACP session are ignored. SDK decoding can discard
invalid entries; missing requested selectors still fail confirmation.

Before each prompt the ACP worker compares the latest observed model/effort to
explicit `session_controls`. A mismatch or missing requested value rejects the
prompt before sending it. An exact valid restoration permits subsequent prompts.
Startup remains a separate immutable snapshot; a notification observed during
setup is never overwritten by an older startup result. With no explicit controls,
manual selection remains usable and reports are informational.

The browser refreshes host memory every 250 ms and displays startup and latest
reports separately, with a warning when a pinned configuration is unconfirmed.
No browser setting mutation, automatic routing, permission or trust change is
introduced. Only the last bounded state is retained, not notification history;
reports are excluded from debug output and SQLite events. Agent reports cannot
attest actual remote inference, detect unreported changes, or guarantee a model
will remain fixed while a turn runs. An already active turn is not automatically
cancelled on drift. A fresh profile launch re-establishes the requested settings.

### Inspecting selectors in the dashboard

The profile editor can explicitly start a prompt-free temporary session to read
bounded model/effort option IDs from a registered agent. Choose a model and
inspect again to refresh its model-dependent effort options; selecting choices
fills the draft without saving or executing a task. Inspecting requires a startup
confirmation and uses denied permissions with durable redacted receipts.
See [the inspection guide](workspace-ui.md#discover-reported-model-and-effort-options)
for limits, startup privacy and unsupported-adapter behavior. These are agent
reports; reviewed execution still confirms exact selectors independently.

The [automatic Jev preview](automatic-jev.md) can choose and dispatch a configured
profile for one submitted task. The mode is experimental and disabled by default.
