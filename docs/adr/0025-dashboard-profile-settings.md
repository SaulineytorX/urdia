# ADR 0025: Explicit dashboard profile settings for the next launch

Status: Accepted (experimental)
Date: 2026-10-02

## Context

Profiles previously required hand-editing TOML. Active recommendations and
confirmation tickets bind an immutable catalog, so changing their definitions
while tasks run would risk executing different settings than the operator reviewed.

## Decision

Expose authenticated, content-free profile/provider metadata in every chat
workspace. Commands, argv, executable paths, workspace paths and credentials are
omitted. Availability means executable discovery, not ACP/account/model validation.

Dashboard writes require an explicit global `--profile-store PATH`. Without it,
the catalog is read-only. The host chooses the path; browser requests contain
only a bounded profile draft, expected revision and make-default boolean.
They cannot register commands, send keys, choose a destination, start an agent,
change permissions or alter trust. Only registered provider IDs are accepted.
The existing profile validator enforces unique IDs, one dedicated launch per
profile, bounded labels, known default, at most 64 profiles and valid control IDs.
API ID fields are bounded to 128 bytes. Labels remain separate from exact pins.

The profile-only store is authoritative when present: it replaces the base
configuration's profile catalog/default, while launch definitions still come
from `--config` and built-in providers. When absent, the first save seeds it
from the validated launch-time catalog. It is never discovered automatically,
and must be separate from provider configuration. The preview launcher forwards
the store to both preflight inspection and the eventual host command.

Saves apply only after restart with the same flags. The primary session, advice,
confirmation tickets and running task settings retain their launch-time catalog.
The dashboard displays pending-restart status and preserves unsaved drafts across
polls. Stored settings can mark a default without executing it immediately.

On Unix, files must be private regular files with a single link; symlinks, hard
links and group/other permissions are rejected. The canonical existing parent
must not be group/other writable. Reads and serialization are bounded to 128 KiB.
Writes use a same-directory temporary file, mode 0600, file sync and atomic
replacement; initial creation refuses an existing destination. A mutex and
revision serialize one process's saves. A private adjacent `.lock` file uses
nonblocking OS locks to serialize cooperating Urdia writers; dropping the handle
releases the lock even after a crash. The empty file remains in place to keep a
stable inode; do not remove it while a host is saving. Source comparison detects
external changes before replacement. This is best-effort change detection, not filesystem
compare-and-swap against non-cooperating same-user writers. Use one writer per
store and restart after external edits. Urdia is not an OS credential sandbox.
Filesystem work runs outside the async server worker; no inference is part of saving.

## Consequences

This provides profile creation/editing for existing registered launches, including
labels, exact model/effort pins and next-launch defaults. New launch registration,
agent installation/login, automatic enumeration of supported model IDs, deletion,
live catalog replacement and secret persistence are separate future work.

No provider-account requests or installed service changes are needed. ADR 0015's
automatic-routing gate and frozen evaluation artifacts remain unchanged.

Update: [ADR 0028](0028-single-restart-onboarding.md) allows setup-only staging
of persisted new launches so agent and profile configuration need one final
restart. Execution configuration remains immutable until that restart.
