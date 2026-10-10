# ADR 0002: Host-owned provider registry

- Status: accepted
- Date: 2026-09-27

## Context

The CLI currently constructs `QwenProvider` directly. That proves ACP transport
and persistence, but it cannot support several agent providers without spreading
provider-specific conditionals through the CLI and future UI.

Urdia also needs to distinguish an installed agent CLI from its authentication
state and from the physical location of model inference. For example, an agent
process may be local while its selected model is hosted.

## Decision

The host will own a `ProviderRegistry` keyed by stable provider IDs. Built-in and
external providers register through the same core contract. Registration rejects
duplicate IDs, and listing is deterministic.

Each provider exposes metadata without starting a session:

- stable ID and display name;
- transport or protocol;
- availability and diagnostic detail;
- authentication state: configured, missing, or unknown;
- inference location: local, remote, mixed, or unknown;
- declared capabilities that Urdia can verify.

Metadata must not claim model locality from the location of the provider process.
Discovery may locate an executable but must not read secrets or start the agent.

The CLI will resolve an explicit `--provider` value through the registry. Custom
ACP entries will use an executable plus an argument array and will never pass a
command string through a shell.

External providers are loaded only from a TOML path explicitly supplied with
`--config`. Each `[[provider]]` entry accepts `id`, `name`, `command`, and
`args`. Unknown fields are rejected. Automatic config discovery and environment
overrides remain out of scope so startup behavior stays visible.

## Consequences

Provider selection becomes independent from CLI code and reusable by a future
UI. Codex, Claude, and custom ACP agents can be added without changing session
orchestration. The registry adds metadata types that must remain conservative:
unknown is preferable to an inferred security or locality claim.

The first implementation will register only Qwen and external ACP definitions.
Provider-specific model selection and credential management remain outside this
ADR.

## Validation

- Reject duplicate IDs.
- Return providers in stable ID order.
- Report missing executables without spawning a process.
- Resolve Qwen through the registry and complete the existing live smoke test.
- Reject unknown provider IDs before creating persisted session events.
