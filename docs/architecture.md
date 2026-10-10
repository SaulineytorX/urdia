# Architecture

Urdia separates execution, decisions, permissions, and persistence.

`ProviderRegistry` resolves a stable, explicit provider ID before any workspace
or database side effect. `AgentProvider` starts an agent session and
`AgentSession` sends prompts and cancellation. ACP is the first transport, but
the core domain does not depend on ACP types.

`DecisionProvider` evaluates bounded semantic choices. Jev, Laya, TypeLLM, and
rules can implement this contract. A decision is data; only
`PermissionBroker` may authorize an operation. Probability semantics are stored
explicitly so calibrated values are not mixed with constrained token
distributions or logits.

`RouteResolver` is the host boundary around a `DecisionProvider`. It accepts
bounded, case-local opaque candidates, adds their English descriptions to
structured state, fingerprints the candidate set, and validates the complete
response. Unknown IDs, incomplete or non-normalized probabilities, invalid
confidence, provider errors, and configured low confidence yield explicit
fallbacks. `urdia route` exposes this recommendation without starting an
agent. Jev is pinned to `jev-1.13.0`. The local Laya adapter uses an
independently authenticated loopback service, targets the isolated `english`
checkpoint, validates the server's routing metadata, and does not claim
calibrated semantics.
The optional TypeLLM adapter uses a separate authenticated loopback bridge to
preserve TypeLLM's schema compilation and constrained decoding over SGLang. It
records constrained-token distributions without inventing calibrated
confidence. Automatic execution is not enabled.

`EventStore` records ordered session events. The SQLite implementation is local
and append-only. Content persistence is a caller policy and defaults off in the
CLI. Permission requests and trusted-session policies remain in memory; only
typed redacted receipts cross the persistence boundary. An allow-once response
is released only after its receipt is stored. Trusted receipts carry a policy
fingerprint and generation so audit history distinguishes their provenance
without turning the receipt into a reusable credential.

The timeline portion of the UI is a read-only projection of this event store.
The static `timeline` command opens an existing SQLite database in read-only
mode, converts event payloads through a small allowlist, and emits one
self-contained HTML file. The live `ui` command reuses that same loader and
renderer behind an explicitly started loopback-only HTTP server. It opens
SQLite read-only, starts no provider, and never exposes raw event payloads.
Unknown fields, workspace paths, retained conversation text, and permission
fingerprints do not reach the rendered document.

`urdia run --permissions-ui` uses the same authenticated loopback server as a
short-lived local control surface. The run creates one
`InteractivePermissionBroker`, gives a clone to the ACP session and the browser
server, and stops the server when the run ends. The browser can inspect only
the safe projection of pending requests and submit one `allow_once` or
`reject_once` decision for an opaque handle. This narrow permission panel is
the only write-like action in the live UI; it does not expose an agent control
plane or raw request details. `urdia ui` remains a historical read-only view
with no active controller.

Adding `--trusted-session` configures a second host-owned lifecycle inside the
same broker, but leaves it inactive. A real pending request supplies the exact
session, provider session, workspace, and revision. The authenticated UI first
prepares a short-lived one-use challenge and then confirms the displayed
provider, workspace, ten-capability allowlist, and 15-minute expiry. Matching
requests can then bypass repeated browser review while ACP continues to emit
only one-shot protocol responses and durable receipts. Expiry and revocation
are revalidated after receipt storage and immediately before release. The
policy is never serialized or restored after restart.

`urdia chat` adds a provider-neutral streaming path. The core
`AgentUpdateSink` receives text deltas without exposing ACP types. A bounded
in-memory `ConversationHub` accepts client-generated turn IDs, allows one
active turn, and projects the live conversation only through the authenticated
loopback API. The CLI remains the session orchestrator: it records lifecycle
metadata, calls the provider, handles cancellation, and decides whether
content is retained. Browser retries are idempotent and conversation content
is not durable unless `--persist-content` is explicit.

```text
surface
   |
session orchestrator
   +-- provider registry -- stable IDs and conservative metadata
   +-- agent provider -- agent-owned loop
   +-- permission broker -- host-owned authorization
   +-- route resolver -- host validation and explicit fallback
   |     `-- decision provider -- optional semantic judgment
   `-- event store -- replayable receipts
```

The two timeline surfaces share the same boundary:

```text
urdia timeline                         urdia ui
       |                                  |
       +---------- read-only loader ------+
                         |
                  privacy projection
                         |
                  rendered timeline

urdia run --permissions-ui
       |
  authenticated loopback browser
       |
  InteractivePermissionBroker <---- ACP permission request
       |
  one-shot review or confirmed trusted policy
       |
  one-shot ACP response + receipt + final revalidation

urdia chat
       |
  authenticated loopback browser
       |
  bounded ConversationHub <---- provider-neutral text deltas
       |
  AgentSession + optional one-shot PermissionBroker
```

The live server binds `127.0.0.1:0` by default, rejects non-loopback
addresses, and authenticates its data and permission routes with a random
in-memory token that is sent as a Bearer header. The token starts in the URL
fragment so it is not sent in the initial HTTP request; the browser clears the
fragment after loading it. Both modes poll through `GET /api/timeline`.
Interactive mode additionally polls safe pending-permission metadata and posts
one exact decision. Its shell and assets are safe to serve without history
because the projection is returned only after authentication.

Both UI surfaces consume Urdia events rather than provider wire messages. This
keeps provider-specific session features available while protecting the
application from protocol churn. The live surface is a local single-user
convenience, not a multi-user service or sandbox boundary.
