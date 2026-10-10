# Provider authoring

Urdia has two extension contracts, and they are deliberately separate:

- An **agent provider** executes coding work through an adapter.
- A **decision provider** returns advisory data about host-prepared candidates.

Neither contract can authorize anything. `PermissionBroker` is the only
authorization boundary, every ACP authorization is one-shot, and a durable
redacted receipt is stored before an allow is released. A decision provider
cannot grant a permission, activate a trusted session, or start an agent.

Read [architecture](architecture.md) and [permissions](permissions.md) before
changing either contract.

## Choose an integration path

| Path | When to use | Urdia code change |
| --- | --- | --- |
| External ACP definition | Any executable that already speaks ACP v1 over stdio. | None. |
| Native adapter crate | Provider-specific discovery, metadata, or deadlines. | New crate plus registry wiring. |
| Decision provider | A new advisory routing or ranking engine. | New crate plus resolver wiring. |

Start with an external definition. It needs no Rust code, and it constructs the
same `AcpProvider` that the workspace contract tests exercise, so it proves an
agent's protocol behavior before any adapter code exists.

## Path 1: external ACP definition

Write a TOML file and pass it explicitly:

```toml
[[provider]]
id = "my-agent"
name = "My ACP Agent"
command = "/absolute/path/to/my-agent"
args = ["--acp"]
```

```sh
urdia --config providers.toml providers
urdia --config providers.toml doctor
urdia --config providers.toml run \
  --provider my-agent \
  --workspace /absolute/path/to/a/repository \
  --prompt "Inspect the project"
```

Rules enforced by the loader:

- Only `id`, `name`, `command`, and `args` are accepted. Unknown fields are
  rejected, and the error names a byte offset rather than the offending key.
- `args` is optional and defaults to empty.
- `id` must be lowercase ASCII kebab-case: letters, digits, and single internal
  hyphens, with no leading or trailing hyphen. Duplicate IDs are rejected,
  including collisions with built-in providers.
- `command` and `args` are passed directly to the ACP transport as an executable
  plus an argument vector. Urdia never constructs a shell command, so shell
  metacharacters have no special meaning.
- A relative `command` is resolved through the Urdia process `PATH`. A path with
  more than one component is used as given and must be an executable file.
- There is no automatic configuration discovery and no environment override. A
  provider is registered only when `--config` names its file.

External definitions report conservative metadata: transport `acp-v1,stdio`,
capabilities `sessions`, `prompt`, and `cancel`, and `unknown` authentication and
inference location. Availability is `available` only when the executable is
found; a missing executable is reported by `providers` and rejected before a
session event is created.

Startup and turn deadlines are not configurable through TOML. External
definitions use the 30-second startup and 300-second turn defaults. Use a native
adapter when a provider needs different values.

Never place a token, password, or other secret in `args`. Command arguments can
be visible to other processes on the host through operating-system process
inspection. Agent CLIs keep managing their own authentication.

## The ACP contract an agent must satisfy

Urdia is an ACP v1 client over stdio using newline-delimited JSON, built on the
official Rust SDK. An agent must:

1. Answer `initialize` with its protocol version and capabilities. Urdia
   completes this handshake before creating a session and stops the child if it
   does not finish within the startup timeout.
2. Answer `session/new` with a provider session identifier. Urdia binds every
   later permission request to that identifier and cancels a request carrying a
   different one.
3. Answer `session/prompt` for each turn. Emit progress as `session/update`
   notifications; Urdia forwards `agent_message_chunk` text to its
   provider-neutral `AgentUpdateSink` and accumulates the same text into the
   final `PromptResult`. Other update kinds are ignored rather than rejected.
4. Finish the turn with a stop reason. Urdia records it verbatim as
   `PromptResult.stop_reason`.
5. Handle `session/cancel` and stop promptly. Cancellation keeps the session
   alive for later prompts when the agent acknowledges it. An agent that ignores
   cancellation is stopped when the turn deadline expires.
6. Send `session/request_permission` before any operation that needs
   authorization, and honour a rejection.

Deadlines:

| Deadline | Default | Behavior on expiry |
| --- | --- | --- |
| Session setup | 30 seconds | `start_session` fails; the worker is shut down. |
| Prompt turn | 300 seconds | The prompt fails, the worker stops, and the connection drop stops the child process. |
| Permission decision | 60 seconds, or the turn timeout if shorter | The request is cancelled and a receipt records the timeout. |

Only one prompt is active per session. A second prompt while one is running is
rejected with `ACP prompt already active`.

## Permission semantics

Urdia maps the ACP `ToolKind` onto its own ten transport-neutral operation
kinds. An absent or unrecognized kind becomes `other`:

| ACP `ToolKind` | Urdia operation kind |
| --- | --- |
| `read` | `read` |
| `edit` | `edit` |
| `delete` | `delete` |
| `move` | `move` |
| `search` | `search` |
| `execute` | `execute` |
| `think` | `think` |
| `fetch` | `fetch` |
| `switch_mode` | `switch_mode` |
| anything else or absent | `other` |

Set the kind accurately. It drives the browser summary, the trusted-session
capability check, and the receipt.

Option kinds matter more than option labels. Urdia searches the offered options
for the kind that matches its decision and selects that option's ID:

- `AllowOnce` for an allow, including an allow authorized by a trusted session.
- `RejectOnce` for a rejection.
- `AllowAlways` is never selected. Offering only persistent options means every
  request is cancelled with a `compatible_option_missing` receipt.

Always offer both an `AllowOnce` and a `RejectOnce` option.

Each request is bound to the Urdia session, the provider session, the canonical
workspace, the prompt revision, the tool-call context, the raw arguments, and the
reported locations. A request whose revision is no longer current, or that
arrives after the turn ended, is cancelled rather than answered.

Default behavior is deny. Without `--permissions-ui`, every request receives
`RejectOnce`, and the run continues so the agent can react to the rejection.
With `--permissions-ui`, a human makes an exact-once decision in an
authenticated loopback panel that receives only an opaque handle and bounded
metadata. With `--trusted-session` additionally confirmed, the host may produce
repeated one-shot allow decisions for 15 minutes — but the protocol response is
still one-shot, and the policy is revalidated immediately before release.

The persisted receipt is redacted by construction. It stores SHA-256
fingerprints of the provider session, workspace, operation, and tool call, plus
the operation kind, whether arguments were present, the location count, the
authorization source, the outcome, and any cancellation reason. Raw arguments,
tool names, titles, and paths are never stored. Do not design an agent that
expects to read back its own request from the event log.

Two practical consequences for agent authors:

- Tool-call arguments stay in memory and are hashed, but they are still sent
  over stdio to the host process. Do not use a permission request to transmit a
  secret.
- An agent must not treat a single allow as a standing grant. Urdia will ask
  again, and a disconnected browser fails closed.

## Offline reference agent

The workspace ships a deterministic ACP v1 agent used by the contract tests. It
is the fastest way to validate an installation, a configuration file, or a
change to the host without an agent account or network access.

```sh
cargo build --release --locked
```

That produces `target/release/urdia-fake-agent`, which accepts:

| Flag | Behavior |
| --- | --- |
| none | Echoes `fake-agent: <prompt>` in one chunk. |
| `--stream` | Splits the response into two ordered chunks. |
| `--request-permission` | Sends one `execute` permission request before responding. |
| `--hang` | Blocks the first prompt until cancelled or the turn deadline expires. |
| `--cancel-aware` | With `--hang`, answers cancellation with a `cancelled` stop reason instead of blocking. |

Register it as an external provider to exercise the complete configuration path:

```toml
[[provider]]
id = "fake-agent"
name = "Urdia Fake Agent"
command = "/absolute/path/to/urdia/target/release/urdia-fake-agent"
args = ["--stream"]
```

```sh
urdia --config fake-agent.toml providers
urdia --config fake-agent.toml run \
  --provider fake-agent \
  --workspace "$PWD" \
  --prompt "hello from the guide"
urdia timeline --database .urdia/events.sqlite --output .urdia/timeline.html
```

The run prints `fake-agent: hello from the guide` and writes lifecycle events
without prompt or response content. Adding `--request-permission` to `args` and
omitting `--permissions-ui` demonstrates the deny-by-default path: the agent
reports `fake-agent: permission rejected` and the run still exits successfully,
because a rejection is a normal outcome rather than a failure.

Never ship the fake agent as a real provider. It exists for contract testing.

## Path 2: native adapter crate

`crates/urdia-provider-qwen` is the reference implementation at about 130 lines.
It wraps `urdia_acp::AcpProvider` and adds executable discovery.

1. Create `crates/urdia-provider-<name>` and add it to the `members` list in the
   root `Cargo.toml`. Inherit `version`, `edition`, `rust-version`, `license`,
   `authors`, and `repository` from the workspace, set a crate-specific
   `description`, and add `[lints] workspace = true`.
2. Depend on `urdia-core` for the contract and `urdia-acp` for the transport.
   The workspace forbids `unsafe_code` and warns on all Clippy lints; CI denies
   warnings.
3. Implement `AgentProvider`:

   ```rust
   #[async_trait]
   impl AgentProvider for MyProvider {
       fn info(&self) -> ProviderInfo { /* no process start, no credential read */ }
       async fn start_session(&self, request: SessionRequest) -> Result<Box<dyn AgentSession>> { /* ... */ }
   }
   ```

   Wrapping `AcpProvider` is usually enough. Implement `AgentSession` directly
   only for a non-ACP transport, and keep ACP types out of `urdia-core`.
4. Report conservative metadata. `ProviderInfo::new` starts with
   `Unavailable`, `Unknown` authentication, and `Unknown` inference. Claim
   `Configured`, `Missing`, `Local`, `Remote`, or `Mixed` only when the adapter
   can verify it safely. `unknown` is preferable to an inferred security or
   locality claim, and finding a local CLI does not prove that its model weights
   run locally.
5. Keep discovery inert. Resolve an executable path; do not start the process,
   read provider configuration, or inspect credentials.
6. Set explicit deadlines with `AcpCommand::startup_timeout` and
   `turn_timeout` when the defaults do not fit the provider.
7. Register the provider in `build_registry` in `crates/urdia-cli/src/main.rs`,
   beside the built-in Qwen registration. Registration order does not affect
   listing order, which is sorted by ID.
8. Add tests, then update the documentation listed below.

`ProviderRegistry` rejects duplicate and malformed IDs, lists deterministically,
and resolves an explicit `--provider` value before any workspace or database
side effect. No provider is ever selected implicitly.

## Path 3: decision provider

Implement `DecisionProvider` from `urdia-core`:

```rust
#[async_trait]
impl DecisionProvider for MyEngine {
    fn id(&self) -> &str { "my-engine" }
    async fn decide(&self, input: DecisionInput) -> Result<Decision> { /* ... */ }
}
```

`DecisionInput` carries an objective, a list of candidate IDs, and structured
state. `Decision` returns an optional selected ID, a probability map keyed by
candidate ID, an optional confidence, and the `ProbabilitySemantics` of those
probabilities: `Calibrated`, `ConstrainedTokenDistribution`, `Logits`, or
`Unknown`.

Rules that are not negotiable:

- Report the semantics your engine actually has. Do not label logits or
  constrained-token distributions `Calibrated`, and do not invent a confidence
  value your engine does not define. Consumers must not compare unlike semantics
  with one shared threshold.
- Select only among the candidate IDs you were given. `RouteResolver` validates
  the response and falls back explicitly on an unknown ID, an incomplete or
  non-normalized probability map, invalid confidence, a provider error, or a
  configured low-confidence result.
- Respect the host bounds: at most 64 candidates, IDs up to 64 bytes,
  descriptions up to 1,024 characters, objectives up to 16,384 characters, and
  state up to 128 KiB. An adapter with a narrower model limit must enforce its
  own cap; the TypeLLM adapter accepts at most 23 candidates because the
  reserved escalation outcome occupies its twenty-fourth slot.
- Treat `__urdia_escalate__` as the reserved escalation candidate when your
  adapter supports escalation.
- Redact errors. Never include a credential, request body, or raw model response
  in an error message, log, event, or evaluation artifact.
- Bind any local service to numeric IP loopback or a Unix socket, authenticate
  it separately, and keep its credential out of TOML, command arguments, and
  logs.

A decision provider is wired into `RouteResolver` and exposed through
`urdia route --router <kind>`. It starts no agent, writes no event, and has no
control path into trusted-session activation.

Production/default autonomous routing requires the preregistered blind evidence
and accepted profile-specific ADR described in
[ADR 0015](adr/0015-automatic-routing-activation-gate.md). Passing an evaluation
does not activate anything by itself. The owner-requested
[automatic Jev preview](automatic-jev.md) is an explicit experimental per-task
exception under ADR0050/0051, preserving host controls and the unpassed production
gate. See the
[evaluation guide](../evaluations/routing/README.md) before changing a prompt,
fixture, metric, or activation policy.

## Required tests

Match the existing coverage rather than adding a single happy path:

- Discovery reports availability without executing the provider.
- A complete turn produces the expected text, ordered streaming fragments, and
  final-result equivalence. See `crates/urdia-acp/tests/fake_agent_contract.rs`.
- Cancellation is idempotent and keeps the session usable.
- A hanging agent is stopped by cancellation or the configured turn deadline.
- An allow is bound to the exact request and its receipt is persisted before the
  response is released.
- Deny-by-default rejects without a UI, and a disconnected browser fails closed.
- Configuration parsing rejects unknown fields, malformed IDs, and duplicates.
- For a decision provider: request and response shape, retry, timeout, response
  bounds, probability normalization, and secret-safe errors.

Run the full set before committing:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
node --check crates/urdia-ui/assets/live.js
git diff --check
```

## Security rules for provider authors

- A provider never grants a permission, activates a trusted session, or widens
  scope. Selection is not authorization.
- ACP child processes run as the Urdia operating-system user and inherit its
  environment. Urdia is not a sandbox. Document that a host with unrelated
  credentials should launch Urdia with a filtered environment.
- Keep secrets out of `args`, TOML, environment dumps, events, receipts, error
  messages, evaluation fixtures, and reports.
- Any listener a provider ships must be numeric loopback or a Unix socket. Do
  not open a firewall port for it.
- Do not persist raw model output when a redacted receipt or aggregate report is
  sufficient.

## Documentation obligations

A completed provider updates every applicable source:

- `README.md` for user-visible commands.
- [providers.md](providers.md) for registry, metadata, and decision-provider
  semantics.
- [installation.md](installation.md) for new prerequisites, environment
  variables, or optional services.
- [permissions.md](permissions.md) when the authorization boundary changes.
- `SECURITY.md` when the attack surface or trust model changes.
- `docs/internal/project-state.md` and `docs/internal/roadmap.md` for the
  verified baseline, and `docs/internal/handoff.md` when server state, secrets
  locations, or the next slice changes.
- A new ADR when the change touches a protocol, durable schema, security
  boundary, or public extension contract. Add a superseding ADR rather than
  rewriting an accepted one.

## Checklist

- [ ] The integration path is the simplest one that satisfies the requirement.
- [ ] The provider ID is lowercase ASCII kebab-case and unique.
- [ ] Discovery is inert and metadata claims only what the adapter verifies.
- [ ] `AllowOnce` and `RejectOnce` options are offered, and no code path expects
      a persistent grant.
- [ ] Deadlines, cancellation, and denial paths are tested.
- [ ] No secret appears in `args`, TOML, logs, events, or errors.
- [ ] Any listener is loopback-only or a Unix socket.
- [ ] Decision providers preserve their own probability semantics and remain
      advisory.
- [ ] Formatting, Clippy, tests, JavaScript syntax, and `git diff --check` pass.
- [ ] User documentation, internal state, and any required ADR are updated.

### Explicit startup configuration

`SessionRequest.controls` optionally requests exact model/effort IDs. The native
ACP adapter validates categorized advertised selectors, applies model before
effort, refreshes and confirms both within the startup deadline; mode/permission
controls are excluded. Other future native adapters must honor explicit requests
or reject startup rather than silently ignoring them. `AgentSession::startup_controls`
returns optional agent-reported initial values; this is not remote model/effort
verification or a promise that settings stay unchanged. See
[profile controls](execution-profiles.md#optional-native-acp-startup-controls).

### Optional selector catalogs

`AgentSession::control_options()` defaults to `None`. An adapter may return only
bounded opaque model/effort IDs and current settings, excluding mode controls,
labels, descriptions and private metadata. The host inspection service validates
unique printable IDs, 128 options per category, current membership and requested
model confirmation before publishing them. ACP derives this view from the same
validated selector parser used for execution. Invalid updates clear its catalog.
Do not advertise labels as executable selectors or a reported catalog as account
entitlement. The explicit inspection path starts a temporary denied-permission
session and sends no prompt; agent initialization may still have native effects.

## Redacted agent failure categories

Prefer `UrdiaError::ProviderFailure(ProviderFailureKind)` for a known agent failure.
It carries only an enum; display/debug output contains fixed host text. Never put
native request bodies, keys, endpoints or response text into it. Unknown errors
must remain unknown, not be guessed into an account/model diagnosis.

The ACP host recognizes auth-required (-32000), known HTTP status in error data,
and an optional strict `data.urdia_failure_kind` string matching the core snake-case
enum. This extension is optional and diagnostic only; ordinary ACP agents do not
need to implement it. Other message/string-data/message fields are scanned only
when bounded to 4096 bytes and containing recognized error phrases; arbitrary
nested prompt/state fields are ignored. Enum metadata is bounded to 32 bytes.
Invalid metadata is never echoed. These are provider reports, not independent
credential checks or access grants.

See [failure recovery](provider-failures.md) for user-facing behavior. Model/effort
confirmation, one-shot permissions, cancellations and explicit retry admission
remain separate. Decision-provider result/error policy is unchanged.
