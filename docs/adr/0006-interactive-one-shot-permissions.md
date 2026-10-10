# ADR 0006: Interactive one-shot permissions

- Status: accepted
- Date: 2026-09-27
- Supersedes: none

## Context

M2 defined a host-owned ACP permission contract with one-shot choices,
revision binding, and redacted receipts. The CLI's default deny-all broker is
safe but leaves a local operator without a way to approve a request while a
run is active. The existing `urdia ui` command is a historical read-only view
and must not silently become an agent control plane.

The browser needs enough information to identify a pending request without
receiving the request itself. A browser response also has to be bound to the
current prompt revision and consumed exactly once. The process must keep the
same loopback-only and non-sandbox guarantees as the existing local UI.

## Decision

`urdia run --permissions-ui` is an explicit opt-in. The run creates one
in-memory `InteractivePermissionBroker` and shares it with the ACP session and
an authenticated `LiveUiServer`. The optional `--ui-bind` accepts a loopback
address only and defaults to `127.0.0.1:0`. The server prints a private URL
whose random token starts in the fragment; the browser moves it to session
storage and sends it as a Bearer token. The server stops when the run ends.

The authenticated `GET /api/permissions` route returns an allowlisted pending
view containing only:

- an opaque random handle;
- the current revision;
- the operation kind;
- whether arguments are present; and
- the number of locations.

It never returns raw arguments, commands, titles, names, paths, workspace,
provider or tool session IDs, or fingerprints. The browser submits a JSON body
with the exact revision and either `allow_once` or `reject_once` to the
matching decision route. A matching resolution removes the pending entry
before delivery, so duplicate submissions have no second effect. Stale
revisions and missing or expired handles are rejected.

The browser shows a short opaque request code and requires a second explicit
confirmation for `allow_once`. The confirmation warns that commands,
arguments, and paths remain hidden and should be accepted only when the
operator independently expects the operation. Rejection remains a single-step
choice.

ACP remains the authorization boundary. It validates the Urdia and provider
session context, current prompt revision, operation, and one-shot option. For
an allow, it persists the redacted receipt before releasing the ACP response;
it never chooses a persistent grant. If the browser is absent or disconnects,
the broker and ACP deadline fail closed. Jev, Laya, TypeLLM, and other AI
decision providers do not authorize the operation.

The standalone `urdia ui` command remains historical and read-only. It has no
active permission controller even though it uses the same authenticated
loopback shell and timeline projection.

## Consequences

- A local operator can answer one active permission request from the browser
  without exposing its sensitive request context to the browser.
- Every allow remains tied to the ACP validation and receipt path from M2;
  the UI cannot grant persistent access or authorize a different request.
- The temporary server and token reduce the lifetime of the control surface,
  but the URL must still be treated as private.
- Exact-once consumption prevents duplicate browser clicks or concurrent
  requests from delivering multiple decisions.
- The feature does not provide a sandbox, operating-system isolation, or
  multi-user protection. A process with the token, loopback access, database
  access, or the same operating-system account may still observe local state.
- Streaming conversation remains a separate M3 slice and must preserve this
  privacy and authorization boundary.

## Validation

- Require explicit `--permissions-ui` for the interactive server and reject
  non-loopback bind addresses.
- Verify the standalone `urdia ui` route remains read-only with no active
  controller.
- Verify the permission snapshot contains only the five allowlisted fields and
  no raw arguments, commands, titles, names, paths, workspace, session IDs, or
  fingerprints.
- Verify `allow_once` and `reject_once` reach the matching pending request,
  while duplicate and stale decisions are rejected.
- Verify ACP still persists a redacted receipt before an allow and ignores
  persistent options.
- Verify a missing or disconnected browser reaches the normal timeout and
  cannot create an allow.
- Verify the transient server stops when the run completes and the loopback
  token is not included in public shell content or debug output.
