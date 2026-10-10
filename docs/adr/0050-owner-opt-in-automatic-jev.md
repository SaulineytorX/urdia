# ADR 0050: Owner-requested automatic Jev preview

Status: accepted for experimental private preview, 2026-10-09.

The owner explicitly selected “Jev elige y ejecuta automáticamente”. This
supersedes ADR0015's disabled-automation requirement only for an explicit,
per-submitted-task experimental browser mode. It does not supersede the production
quality gate, alter frozen corpus evidence or establish semantic routing quality.

The host exposes configured profile dispatch capability independently of the
latest recommendation. Browser-connected Jev can compare existing available
profiles without loading external credentials or calling a model during startup.
Manual sending is default. Selecting the mode and submitting a visible task
authorizes that task's remote decision and immediate valid-profile dispatch.

Reuse the authenticated advisory admission, host-owned five-minute preparation
ticket and idempotent execution endpoints. Fresh selector application, configured
target availability, fallback, busy, stale and plan-step checks remain mandatory.
The model returns a typed preference; host code owns execution and permissions.
No new credentials, targets, selectors or permission rules come from model output.

The tab retains a single in-memory decision/task pair. Interrupted deliveries
require explicit same-ID retry; no blind retry or fallback model invocation.
Closing/reloading clears the chain, without cancelling an admitted task. No
automatic multi-step loops, concurrent jobs or trusted-session activation.

Production/default autonomous routing still requires a fresh evaluation gate.
The private preview must label this mode experimental and disclose quota use,
scope and the known incomplete quality evidence.
