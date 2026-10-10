# ADR 0003: One-shot permission receipts

- Status: accepted
- Date: 2026-09-27

## Context

ACP v1 permission requests carry a provider session ID, a partial structured
tool call, raw JSON input, reported locations, and a set of choices. The choice
set can include persistent grants. Raw input can contain commands, paths, or
credentials, while ACP tool kinds alone are not a verified capability model.

Urdia needs to support future user authorization without letting provider
routing, semantic models, stale decisions, or durable logs expand authority.

## Decision

Each agent session receives a host-owned `PermissionBroker` and
`PermissionReceiptSink`. The ACP adapter binds a request to the Urdia session,
negotiated ACP session, canonical workspace, current prompt revision, tool-call
context, arguments, and locations. The broker sees this exact in-memory context
and can return only `AllowOnce` or `RejectOnce`.

The adapter selects only an ACP option whose kind matches that one-shot
decision. Persistent options are ignored. It fails closed on mismatched
sessions, inactive or stale revisions, broker errors or timeouts, missing
compatible options, and receipt persistence errors.

The request is intentionally non-serializable and redacts raw arguments from
debug output. Its durable receipt stores decision metadata and SHA-256
fingerprints of sensitive binding context. It omits raw arguments, titles, tool
names, provider session IDs, workspace paths, tool-call IDs, and locations. A
receipt must be stored before an allow-once response is sent.

The default sink rejects allowed receipts. A response gate revalidates prompt
activity and revision after persistence and is held through the ACP response,
so cancellation or a later prompt cannot race a stale allow into use.

The permission callback moves policy and persistence work outside the ACP
dispatch loop so later protocol messages can continue to flow. The CLI supplies
a SQLite receipt sink and retains the deny-all broker until an interactive
surface is implemented.

## Consequences

- A decision cannot become a persistent grant through ACP option ordering.
- Stored events can correlate the operation that was considered without copying
  its sensitive values.
- SHA-256 fingerprints are audit evidence only and never capabilities.
- Allow-once does not sandbox the ACP child, which still runs as the current OS
  user with the inherited environment.
- A future UI can inject a broker per session without changing provider
  adapters or involving a semantic decision provider.

## Validation

- Unit tests select the matching one-shot option even when a persistent option
  appears first.
- Missing one-shot options and receipt failures cancel instead of allowing.
- A fake ACP agent completes a real request/response turn through allow-once.
- Sentinel secrets are visible to the broker but absent from serialized
  receipts and SQLite events.
