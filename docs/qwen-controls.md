# Qwen model and effort control evidence

Urdia's profile labels remain operator declarations. An optional reproducible
probe now checks Qwen Code 0.24.7's actual ACP session configuration selectors,
changes a supported effort and switches between two fixture-configured models.
It does not run a prompt, verify inference, measure model quality or enable
automatic routing. It does not change your Qwen configuration.

## Run the offline probe

Inside Linux/WSL with Qwen Code 0.24.7 installed:

```sh
python3 -I -B scripts/check-qwen-controls.py --qwen "$(command -v qwen)"
```

If multiple installations or wrappers exist, select the exact trusted CLI and
its Node runtime explicitly:

```sh
python3 -I -B scripts/check-qwen-controls.py \
  --qwen /absolute/path/to/qwen-code/cli.js \
  --node /absolute/path/to/node
```

The tool checks the reported version before session creation and rejects other
versions. Use a trusted executable: the tool runs installed provider code as your
OS user; it is not a sandbox. No Qwen installation, model download, server service
configuration or user preferences are modified.

The probe creates a disposable HOME/project and its own two-model configuration,
uses a placeholder credential and points inference at a rejecting loopback HTTP
fixture on an ephemeral port. Only PATH and the necessary fixture environment
are passed; real credential variables are not inherited. It sends initialize,
session/new and supported session/set_config_option requests, never session/prompt.
It advertises no filesystem or terminal services, cancels permission requests
and rejects other agent-to-client requests. Modes/permission controls are omitted
from the selector report and cannot be selected by the control helper.

Responses are bounded to 512 KiB / 256 messages with a 45-second ACP deadline;
version checking has a separate 10-second limit. Agent processes are stopped
with a bounded termination/kill sequence; fixtures and disposable directories
are removed. Raw stderr, errors, descriptions, metadata, session IDs and paths
are omitted from output. Only bounded model/thought selector IDs and values are
reported. A failure prints a constant diagnostic.

## Observed result

The installed 0.24.7 CLI passed this sequence on the maintainer server:

| Fixture model | Agent-advertised thought values | Checked behavior |
| --- | --- | --- |
| `qwen3.8-max(openai)` | `none`, `low`, `medium`, `xhigh` | Initially xhigh; setting medium returned medium. |
| `qwen3.7-plus(openai)` | `none`, `default` | Switching model returned default; medium was rejected locally before another RPC. |

Those are **agent-reported settings under an offline fixture**, not a claim that
Alibaba accepted these options or that a model ran with them. The observed run
sent zero prompts and made zero requests to the fixture. No real inference or
billing was involved. Different provider configurations can offer different
selectors: always use the IDs/values currently advertised by the agent, and
refresh the full option list after changing model. Do not apply a universal
low/medium/high ladder or treat mode controls as effort.

Two server installations were found: the regular launcher reported 0.24.7,
while an older NVM package reported 0.24.4. The recorded passing run used the
explicit 0.24.7 package/runtime, not that older installation. No installation
was upgraded or removed.

The [ACP configuration contract](https://agentclientprotocol.com/protocol/v1/session-config-options)
explains complete option lists returned after a change. Qwen's
[versioned implementation](https://github.com/QwenLM/qwen-code/blob/v0.24.7/packages/cli/src/acp-integration/model-configuration.ts)
is the source reference for its model-dependent reasoning controls.

## What this enables next

The running Urdia ACP host does not yet expose or change these session selectors.
This probe supplies evidence and regression coverage for that next integration.
Keep declared profile labels separate from agent-reported settings, and both
separate from inference-provider verification and measured task outcomes.
`model_effort_verified` in existing profile UI/advice remains false. Jev/Laya
quality, a new frozen profile evaluation, and ADR 0015 activation evidence still
need separate work. Claude/Codex controls have not been verified by this probe.

## Native Urdia startup checkpoint — 2026-10-02

The Rust ACP host now discovers categorized selectors and optionally applies
explicit `session_controls` from a launch profile. The workspace shows the
agent-reported initial model/effort separately from display labels. See
[execution profiles](execution-profiles.md#optional-native-acp-startup-controls).

A maintainer native-host check used the exact installed Qwen 0.24.7 CLI/runtime,
a disposable HOME/project and the same rejecting loopback fixture pattern:

- `qwen3.8-max(openai)` + `medium`: startup confirmed in authenticated context.
- `qwen3.7-plus(openai)` + `none`: startup confirmed after changing model.
- `qwen3.7-plus(openai)` + `medium`: startup rejected before a prompt.

Zero prompts, zero fixture endpoint requests and no real credentials were used.
Private sanitized evidence is outside the repository at
`<private acceptance artifact omitted>`.
This actual-host check was a temporary maintainer harness, removed afterwards;
the checked-in repeatable native contract tests use the offline Rust reference
agent, and the separate Python probe above still exercises real Qwen controls.
The optional browser acceptance covers declared versus reported settings.
These checks do not prove remote inference or permission handling on the user's
PC and do not unlock automatic routing.
