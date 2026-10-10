# ADR 0023: Session-only browser configuration of local Laya

- Date: 2026-10-02
- Status: Accepted

## Context

Local decision setup required environment variables and a host restart. The owner
wants configuration in the product. Connecting an existing service is independent
of installing software and enabling automatic agent execution.

## Decision

Expose an authenticated POST `/api/advisory/laya` with closed connect/check/disconnect
operations and content-free GET snapshot metadata. Browser entry is allowed for
this local service key: submit it to the authenticated loopback Urdia host, clear
the field, keep it only in the host adapter, and never return it or write it to
logs, browser storage, project configuration, SQLite or the agent environment.
No persistent secret store or secure-erasure guarantee is introduced.

Default chat offers a disconnected local Laya adapter without loading environment
keys or enabling hosted adapters. An explicit connect/test action activates that
session's advisory adapter after validation; `--advisory-ui` remains necessary to
use other host-configured engines. Numeric IP loopback endpoint restrictions,
disabled redirects/proxies and response/input bounds remain in the Laya adapter.

Connection testing uses a small fixed typed decision against the authenticated
English endpoint. The pinned server's `/health` is unauthenticated and cannot
prove the key. A successful test confirms a compatible endpoint response, not
recommendation quality, ongoing availability or permissions. Preserve old
configuration on failure; disconnect drops its in-memory adapter.

Serialize configuration with advisory requests; enforce the existing 35-second
host deadline, bounded input and 16 tests per process. Release the busy slot on
cancellation/failure and retain disconnect after the test cap. Never call inference
on polling/view opening. Redact parse/validation/upstream errors.

## Consequences

Browser setup needs no terminal variables or restart for installed Laya. It lasts
until host shutdown and survives browser refresh. Installing a missing service,
checkpoint downloads, privileged operations and persistent key storage remain
separate work. A service on a remote server is not a local WSL installation.
Same-user agent/OS access is not isolated by this feature.

Advice remains distinct from execution and permission policy. ADR 0015 stays in
force and frozen routing evaluation artifacts are unchanged. Offline API/state
regressions and a loopback fixture browser flow cover authentication, key omission,
replacement failures, recommendation availability and disconnect.
