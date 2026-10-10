# ADR 0005: Authenticated loopback live UI

- Status: accepted
- Date: 2026-09-27
- Supersedes: none

## Context

The static timeline export is useful for a snapshot, but a local operator also
needs to see newly recorded sessions without regenerating a file. A browser
surface introduces a listening socket and a way to distinguish a local caller
from an unauthenticated request. Urdia still has no sandbox or multi-user
isolation, and the live surface must not turn a read-only history view into an
agent control plane.

## Decision

`urdia ui` starts an opt-in live timeline server on an IP loopback address. The
default bind is `127.0.0.1:0`, so the operating system chooses an ephemeral
port. The implementation rejects non-loopback addresses in code. The command
prints one private access URL containing a random 64-hex-character token in
the URL fragment. The fragment is not included in the browser's initial HTTP
request; the token remains in process or browser-session memory and is sent
only in the authorization header for the protected endpoint.

The browser shell reads the fragment once, stores the token in
`sessionStorage`, clears the fragment from the address bar, and sends the token
as `Authorization: Bearer ...` for `GET /api/timeline`. The shell and its CSS
and JavaScript assets contain no event data and can be fetched without the
token. The data route requires the token and returns the existing
privacy-filtered rendered HTML projection, never raw event JSON or unfiltered
payloads. The browser polls that route every two seconds.

The live server opens the SQLite event database read-only. Its routes are
read-only GET routes: it has no provider actions, permission decisions, CORS
surface, or mutation endpoint. Responses set no-store, a restrictive content
security policy, no referrer, MIME-sniffing protection, and frame protection.
The process stops on `Ctrl-C`.

## Consequences

- The operator gets a live local history view without regenerating an export.
- Port `0` avoids a fixed-port collision and the loopback check prevents an
  accidental LAN bind through the CLI.
- The fragment keeps the token out of the initial request and normal browser
  referrer data; the printed URL still has to be treated as private.
- A local process or user that can access the database, loopback listener, or
  token may access the view. This is not multi-user isolation or a sandbox.
- The same projection code serves static and live views, so retained prompt or
  response content, workspace paths, fingerprints, unknown fields, and raw
  provider payloads remain excluded.
- Interactive permission handling and streaming conversation remain later
  slices; this UI cannot start an agent or authorize a tool call.

## Validation

- Reject non-loopback bind addresses before opening a listener.
- Bind `127.0.0.1:0` and report the actual ephemeral port without exposing the
  token through debug output or server logs.
- Return the public shell and assets without history or token data.
- Return `401 Unauthorized` for a missing or incorrect Bearer token and the
  allowlisted timeline projection for the correct token.
- Open SQLite read-only and keep the live route GET-only.
- Verify no secret sentinel from prompt, response, workspace, fingerprint, or
  unknown event fields reaches the live HTML.
- Verify security headers and graceful `Ctrl-C` shutdown in focused tests or a
  local smoke run.
