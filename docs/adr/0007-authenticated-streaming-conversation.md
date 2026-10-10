# ADR 0007: Authenticated streaming conversation

- Status: accepted
- Date: 2026-09-27
- Supersedes: none

## Context

The one-shot `urdia run` path waits for a complete provider response. M3 also
needs a local conversation where an operator can submit several sequential
prompts, see text while the provider is producing it, cancel an active turn,
and answer the existing one-shot permission requests.

Provider wire messages must not leak into the UI contract. Browser retries
must not execute a prompt twice, and conversation content must retain the
existing default of no durable storage.

## Decision

`AgentSession` exposes a provider-neutral `prompt_with_updates` method. Its
synchronous, non-blocking `AgentUpdateSink` receives text deltas while the
existing `PromptResult` remains the authoritative completed result. The ACP
adapter owns the active sink, drops updates after the turn ends, and keeps a
fixed deadline for the complete turn.

`urdia chat` starts one provider session and an authenticated loopback UI. The
default address is `127.0.0.1:0`; non-loopback addresses are rejected. The
conversation and optional `--permissions-ui` broker live in the same process
as the provider session. `Ctrl-C` cancels active work, records the session end,
and stops the server.

The browser submits a client-generated UUID with each prompt. Repeating the
same UUID and text returns the existing turn without sending a second host
command. Reusing it with different text is rejected. One turn may be active at
a time and the bounded host command channel applies backpressure.

The authenticated API returns a sequence-numbered, bounded in-memory snapshot.
The browser polls it every 500 milliseconds while conversation mode is active
and renders all agent-controlled text through `textContent`. Prompts are
limited to 16,384 characters, responses to 65,536 characters, and history to
64 visible turns. Urdia retains at most 1,024 accepted prompt identities for
session-wide replay protection, then requires a new chat session. The shell
and static assets contain neither the access token nor conversation data.
Authorization middleware runs before JSON extraction.

Prompt and response content is written to SQLite only with
`--persist-content`. Without it, durable events contain turn IDs, character
counts, lifecycle state, and stop reasons. The authenticated in-memory view
still contains the live conversation and its private URL must be protected.

## Consequences

- All current and future providers can stream through one Urdia contract.
- Browser reloads recover the bounded session view, while process exit erases
  conversation content that was not explicitly retained.
- Polling keeps authentication in the Bearer header and avoids putting tokens
  into an EventSource URL. WebSockets or another transport can replace it
  later without changing the provider update contract.
- Conversation mode is a local single-user control surface, not a sandbox or
  a multi-user service.
- Cancellation depends on the provider honoring ACP cancellation within the
  turn deadline. Urdia discards subsequent text from the cancelled turn.

## Validation

- Verify ordered ACP chunks reproduce the final result and do not cross turns.
- Verify a cancelled prompt can be followed by a new prompt in the same ACP
  session.
- Verify prompt UUID idempotency, conflict, busy state, bounded queue, strict
  JSON, content limits, and monotonic state transitions.
- Verify conversation content requires the Bearer token and is inserted with
  safe DOM text APIs.
- Verify standalone `urdia ui` remains historical and has no prompt endpoint
  state even though it serves the shared shell.
- Verify non-loopback binds fail and the default port is ephemeral.
- Verify content remains absent from SQLite unless retention is explicit.
