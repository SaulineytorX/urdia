# ADR 0026: Explicit next-launch agent registration

Status: Accepted (experimental)
Date: 2026-10-02

## Context

The profile editor can only select existing registered launches. Requiring a
handwritten provider TOML for each additional agent interrupts onboarding.
Registering a launch introduces executable configuration and must remain distinct
from permissions, semantic decisions and active session settings.

## Decision

A global `--agent-store PATH` explicitly enables a private launch-only store and
its authenticated dashboard registration API. Without this flag registration is
read-only. The host selects the fixed destination, separate from provider config
and the profile store, including absent path aliases and existing hardlinks.
The store uses the profile store's bounded reads, private parent/file checks,
atomic replacement, source-change detection and cooperating-process lock.

Registration accepts a new ID, bounded display name, executable and explicit
argument array, expected revision and required trust confirmation. No shell
command string is constructed, interpolation performed, executable launched,
installation performed or inference requested during registration. A user can
choose an interpreter as their executable; direct argv does not make that safe.
The operator must trust the complete launch definition.

There are at most 64 stored launches, at most 64 arguments per launch and 128 KiB
per file. IDs/names are bounded to 128 bytes and executable/arguments to 4096
bytes; control characters, duplicate IDs and unknown fields are rejected.
No environment or credential fields are accepted. Arguments are not a credential
vault: users must keep secrets out of them. Native agents manage their own login.
A successful save clears executable/argument inputs in the browser. Snapshot
responses omit both; drafts remain only in DOM memory before saving or on error.

Stores are loaded before profile validation on the next launch. Duplicates across
built-ins, provider config and the store fail closed. Saving only adds a pending
registration; the active registry, profiles, recommendation candidates, tickets
and running tasks remain unchanged. Restart with the same flags, then create an
execution profile for the new launch. Executable discovery is not evidence of
ACP, authentication, model availability or execution quality.

## Consequences

The browser can register trusted installed ACP agents without editing TOML.
Qwen has a dedicated-launch preset; generic agents and the optional Codex bridge
use an explicit executable and argv. Claude needs an installed compatible ACP
adapter. No provider install/login or credential copying is automated.

This slice supports addition, not editing/removing registered commands. Model and
effort discovery, service installation, live registry mutation, credential
persistence and automatic orchestration remain separate work. ADR 0015 and the
frozen evaluation artifacts are unchanged. Urdia is not an OS sandbox.

Update: [ADR 0028](0028-single-restart-onboarding.md) allows setup-only staging
of persisted new launches so agent and profile configuration need one final
restart. Execution configuration remains immutable until that restart.
