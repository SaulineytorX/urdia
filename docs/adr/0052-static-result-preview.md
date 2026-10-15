# ADR 0052: Explicit static-result previews on a fresh browser origin

- Status: accepted for source preview
- Date: 2026-10-15

## Context

An agent can create a working visual project, but requiring its owner to start a
server in a terminal interrupts the first-result experience. The main Urdia
origin contains authenticated controls and must not host arbitrary project code.
A random port alone is insufficient: port reuse can restore older origin storage.

## Decision

Expose an authenticated, deny-unknown-fields preparation request bound to the
latest completed turn and explicit browser-code consent. The host uses its fixed
workspace, never a client path, command, URL or build instruction. Preparation
runs no AI, dependency manager or project command. Serialize admission with tasks,
verification, reconnect and profile changes; ready previews do not block tasks.
Queue failure must not mutate state. Same-ID retry is idempotent, terminal IDs
cannot restart, and superseded IDs return stale. Limit to 16 requests/session.

On Linux, walk retained directory file descriptors from the filesystem root,
reject symlinks at every component, open selected regular files with no-follow,
reject hard links, and require bounded reads and matching file metadata. Require
two identical bounded captures. This detects ordinary drift; it is not an atomic
filesystem snapshot or evidence of exact post-task authorship. Reject unsupported
source layouts conservatively, including unignored symlinks. See the user guide
for the file, tree and memory limits.

Serve immutable allowlisted bytes only on a dedicated numeric-loopback listener,
with a fresh random `p-<48 hex>.localhost` Host capability and ephemeral port.
Require that exact Host, serve only GET/HEAD, and expose no Urdia API, filesystem
path handler, CORS or parent bearer. Open explicitly in a separate tab with
`noopener noreferrer`. Reserved localhost handling follows
[RFC6761 section 6.3](https://www.rfc-editor.org/rfc/rfc6761#section-6.3).

Use no-store/referrer/MIME protections, disabled browser permissions, COOP/CORP
and a restrictive [CSP](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy).
Allow inline scripts/styles for ordinary static outputs and same-origin assets;
prohibit frames, objects, workers, form navigation and external fetch resources.
This is browser-code execution, not OS containment or complete network isolation.
Require trusted code; navigation and already-loaded code remain outside link
revocation. Local peer processes can access a known private link.

One 15-minute lease is active at a time; replacement, stop, host shutdown and
expiry invalidate it. Expiry must update even without a conversation event.
Stopping a listener does not close its tab or undo browser execution. Each new
origin starts separate storage; export before replacement. Mark old-turn views
explicitly, and distinguish capture from independent project verification.

## Consequences

The first result can be used without a terminal command, at the cost of temporary
browser storage and a deliberately narrow static-project contract. Framework
builds, backends, remote APIs and native Windows/macOS capture require separate
future designs. No preview credential or bundle is persisted to event history.

## Validation

Host tests cover admission races, consent, shared ID conflicts, terminal replay,
limits and stale host starts. Reader tests cover links, ancestors, nonregular
files, size/count and immutable captures. HTTP tests cover Host/method/path,
headers, MIME and expiry. Optional sandboxed-browser acceptance uses the actual
previously generated Launchboard files and an explicitly offline completion
fixture, with zero new model calls: desktop/390/320, create/reload/move/export,
fresh-origin token/storage/cookie separation, replay, replacement and stop.
