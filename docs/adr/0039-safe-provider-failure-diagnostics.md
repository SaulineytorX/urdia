# ADR 0039: Fixed provider failure diagnostics

- Status: accepted
- Date: 2026-10-05 UTC

## Context

ACP error messages may contain credentials, endpoints, source text or native logs.
The browser deliberately showed only a generic failed-turn message. A recent
Qwen handoff failure required a test-only proxy to identify authentication; users
could mistake executable discovery for successful account authentication.

## Decision

Introduce a provider-neutral `ProviderFailureKind` and typed core error retaining
only that enum. The ACP transport sanitizes initialize/session/prompt/cancel RPC
errors before forwarding them. Prefer ACP auth-required code, allowlisted bridge
metadata and structured HTTP status; otherwise inspect bounded known message
fields for narrow provider error phrases. Never scan arbitrary nested state or
retain source error bodies. Ambiguous/unrecognized input stays unknown. Host-owned
startup/queue/turn deadlines become typed timeout errors.

The pinned Codex 0.159.2 bridge maps only structured `codexErrorInfo` variants and
HTTP status from native failed turns/RPC errors into fixed ACP error metadata.
It never forwards native messages or additional details. Scope remains exact
thread/turn; a failed bridge's later rejected request is unknown rather than an
inherited stale account diagnosis. Ordinary completion/cancellation ordering and
permission handling do not change. Native/provider internal retries are outside
this layer; Urdia adds none.

Chat uses static host-authored explanations and records only stage, turn UUID,
category and details-persisted=false for prompt errors. Plain/untyped core errors
remain generic rather than being parsed again in the UI. Fresh-profile startup
failures show a recognized kind, or the existing exact-settings message when
unknown. History renders only recognized enum values; old events stay compatible.
The live task panel displays the same safe host message as the transcript. A
failed fresh-session setup may have no target lifecycle record because no session
was created; do not attribute it to a fabricated target session.

Diagnosis is not verified account state, successful execution or permission.
Categories cannot grant access, mutate profiles/credentials, select fallback,
start a retry or open ADR0015's automatic routing gate. Discovery metadata remains
unchanged. A dead original session still needs explicit reconnection or a fresh
profile; do not imply a retry alone fixes an external account problem.

## Consequences

Provider-native bodies are not available through these core errors, events or UI,
even when content retention is enabled. Fixed vocabulary trades detail for privacy.
Bounded phrase inference may misclassify an ambiguous provider report; wording and
documentation distinguish reports from verified facts. No billing amounts, retry
backoff, credential validity or remote model identity are inferred.

## Validation

Offline tests cover priority/type/bounds/unknown fields, native error privacy,
scoped Codex final-failure propagation through the real ACP SDK, stale diagnosis,
one prompt per host turn, redacted persistence, unknown timeline metadata and
explicit browser retry only. Browser fixtures exercise authentication, usage/rate,
model, connection and unknown failures in sidebar/transcript/history and mobile.
No real provider or model request is needed for this change.

The current [App Server error documentation](https://learn.chatgpt.com/docs/app-server)
and installed 0.159.2 generated schemas were checked. The pinned schemas use camel
case structured variants; newer runtime support is not implied.
