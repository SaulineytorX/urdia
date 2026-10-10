# Providers

## Agent providers

Agent providers perform coding work. The first adapter launches Qwen Code with
`qwen --acp` and negotiates ACP v1. The optional [native Codex bridge](codex.md) adds explicit Codex execution through
its installed app-server. Planned adapters include Claude Code through its ACP
adapter and a Urdia agent loop for OpenAI-compatible local inference servers.

`ProviderRegistry` owns providers by stable lowercase kebab-case ID. Duplicate
or invalid IDs are rejected, and listings are ordered by ID. Session commands
can select an explicit ID, for example:

```sh
urdia providers
urdia run --provider qwen --workspace /repo --prompt "Review this project"
```

Provider metadata distinguishes availability, authentication, and inference
location. `unknown` is intentional: finding a local CLI does not prove that it
is authenticated or that its model weights run locally. Discovery does not
start the agent or inspect credentials.

## External ACP providers

Pass an explicit TOML file with `--config`:

```toml
[[provider]]
id = "my-agent"
name = "My ACP Agent"
command = "/absolute/path/to/my-agent"
args = ["--acp"]
```

The accepted fields are `id`, `name`, `command`, and `args`. Unknown fields are
rejected. Urdia passes the executable and argument array directly to the ACP
transport; it never constructs a shell command. Relative executable names are
resolved through the Urdia process `PATH`.

Do not put tokens or passwords in `args`: command arguments may be visible to
other processes on the host. Provider CLIs should keep managing their own
authentication. External provider authentication and inference location remain
`unknown` until a dedicated adapter can verify them safely.

The [provider-authoring guide](provider-authoring.md) documents the complete ACP
v1 contract an external agent must satisfy, including permission option kinds,
deadlines, cancellation, and the offline reference agent used to test it.

An installed CLI and locally hosted model weights are different capabilities.
Provider manifests will eventually report authentication, modalities, model
configuration, context limits, reasoning controls, and whether inference is
local or remote.

## Dashboard registration

The [dashboard onboarding flow](workspace-ui.md#register-an-installed-agent-in-the-dashboard)
can add trusted installed ACP launches with an explicit private `--agent-store`.
It saves executable/argv for the next launch without starting the agent. Existing
`--config` registration remains supported. IDs must be unique across all sources;
native authentication remains with each agent. Saved new entries become setup-only profile choices immediately. Configure
their profiles and restart once with both stores to activate them for tasks.

## Execution profiles

Named profiles and a configured default are available for `run` and `chat`.
See [execution profiles](execution-profiles.md) for configuration, selection
precedence and the distinction between labels and verified model settings.

## Decision providers

Decision providers choose among candidates already admitted by the host.

- Jev: hosted typed judgments and documented confidence semantics.
- Laya: optional local System One compatible judgments.
- TypeLLM: optional constrained generation over an independently operated
  SGLang server.
- Rules: deterministic offline selection.

TypeLLM is not a core dependency. Urdia's optional adapter calls an
authenticated bridge on `127.0.0.1:18767`; that bridge runs TypeLLM 0.2.6
against a separately operated numeric-loopback SGLang service. The adapter
requires `URDIA_TYPELLM_MODEL` to name the exact served model as
`model@revision`,
`URDIA_TYPELLM_API_KEY` or the systemd `typellm_key` credential, and accepts at
most 23 host candidates because the reserved escalation outcome occupies
TypeLLM's twenty-fourth enum slot.

TypeLLM distributions are recorded as `ConstrainedTokenDistribution` with no
confidence value. They must not be labeled calibrated or compared with Jev or
Laya thresholds. The bridge uses temperature zero, one ordering, no requested
thinking, and English routing instructions. It starts no agent and has no
authorization authority.

The optional files under `deploy/typellm/` pin and verify the TypeLLM 0.2.6
wheel, install a hardened systemd unit, and deliberately leave it disabled.
Before enabling it, an operator must serve an immutable model through SGLang on
loopback and copy `typellm.conf.example` to `/etc/urdia/typellm.conf` with that
exact model identifier and an absolute local tokenizer directory. Runtime
downloads are disabled. Both SGLang and the bridge must remain private. The
current Urdia host has neither supported accelerator hardware nor SGLang, so no
TypeLLM service or live evaluation is active here.

The official Laya package includes a `/v1/systemone` server compatible with
the same typed request shape. Urdia's separate Laya adapter pins requests to
the `english` checkpoint, requires bearer authentication, accepts only a
numeric loopback endpoint, and validates that the server actually used that
model. Its probability semantics remain `Unknown`; Jev thresholds do not carry
over.

The current optional deployment under `deploy/laya-english/` installs Laya
0.3.20 with CPU-only PyTorch and the immutable English Hugging Face revision.
It loads only that local checkpoint, listens on `127.0.0.1:18766`, uses the
separate root-owned `/etc/urdia/secrets/laya-english.key`, runs as a dynamic
user, and cannot reach non-loopback addresses at runtime. Install it explicitly
with:

```sh
sudo ./deploy/laya-english/install.sh
```

The earlier typed-decisions service under `deploy/laya/` remains intact on
`127.0.0.1:18765` as a historical and rollback artifact. The English adapter
targets the isolated service on port 18766.

The adapter reads `URDIA_LAYA_API_KEY`, or `laya_key` from systemd's
`CREDENTIALS_DIRECTORY`. `URDIA_LAYA_URL` may override the default endpoint,
but the value must still be a numeric loopback URL. Do not place either value
in repository configuration or command arguments.

The first implemented adapters are deterministic first-candidate selection and
TypeSafe Jev. The Jev adapter pins `jev-1.13.0`, sends an English Choice
question, and reads `TYPESAFE_API_KEY` from the process environment. When run
under systemd, it can instead read the `typesafe_key` credential from
`CREDENTIALS_DIRECTORY`. Urdia does not accept the key in TOML or a command
argument.

To inspect the host boundary without contacting a remote service:

```sh
urdia route \
  --objective "Review this Rust workspace" \
  --router deterministic \
  --candidate qwen
```

To request a Jev recommendation, deliberately expose the key only to that
process and select `--router jev`. The objective, optional JSON state, and
prepared candidate descriptions are sent to TypeSafe. The returned route is
advisory: this command starts no agent and grants no permission.

```sh
urdia route \
  --objective "Review this Rust workspace" \
  --router jev \
  --candidate qwen
```

Use `urdia eval-routing` before enabling a decision provider in automation.
[ADR 0015](adr/0015-automatic-routing-activation-gate.md) defines the required
blind evidence and safety gate. No current provider profile passes it, so
production/default autonomous routing remains disabled. The owner-requested
[automatic Jev preview](automatic-jev.md) is an explicit experimental per-task
exception; it does not establish a passing gate.
Its deterministic mode is offline. Jev evaluation additionally requires
`--live`, and the fixture must pin the exact adapter model.

Laya evaluation is local and uses the matching fixture metadata:

```sh
sudo systemd-run --wait --pipe --collect \
  --property=LoadCredential=laya_key:/etc/urdia/secrets/laya-english.key \
  --working-directory="$PWD" \
  target/debug/urdia eval-routing \
  --fixture evaluations/routing/v2-laya-english.json \
  --router laya \
  --split held-out \
  --repeats 3
```

No semantic provider has a passing production/default autonomous-routing gate.
The experimental Jev task mode is separately documented above.

## Authoring and installing

- [Provider authoring](provider-authoring.md): external ACP definitions, native
  adapter crates, decision providers, required tests, and the security rules for
  each.
- [Installation](installation.md): prerequisites, building from source,
  configuration and environment variables, optional local decision services,
  upgrades, backups, rollback, and troubleshooting.

## Browser advice

`urdia chat --advisory-ui` exposes explicit recommendations through the same
validated adapters and host-prepared available registry candidates. Adapter
configuration is loaded at startup; a configured label is not connection or
quality verification. Browser requests cannot supply keys or endpoints. See the
[workspace guide](workspace-ui.md) for source-build setup, remote consent, limits
and the distinction between recommending an agent and executing a task.

An installed English Laya service can now be connected and tested in the browser
without environment variables or a host restart; see [workspace setup](workspace-ui.md#choosing-decision-intelligence).
The connection is session-only and remains advisory.

Jev also supports [explicit browser connection and OS key storage](jev-credentials.md),
without putting credentials in TOML/argv or starting an agent.
