# Permissions

Selection and authorization are separate operations. A router may choose an
agent or tool focus; that choice never grants filesystem, process, or network
access.

The ACP adapter converts each request into a transport-neutral structure bound
to the Urdia session, the negotiated ACP session, the canonical workspace, the
current prompt revision, the tool-call ID, its kind, raw arguments, and reported
locations. Raw arguments are available only to the in-memory
`PermissionBroker`; the request type cannot be serialized and its debug output
redacts them.

A broker returns a one-shot allow, one-shot rejection, or a host-only trusted
allow carrying opaque in-memory policy provenance. The adapter always selects
an ACP one-shot option and reuses its exact option ID, including for a trusted
allow. It never selects `allow_always` or `reject_always`. A session mismatch,
missing one-shot option, stale revision, broker failure, timeout, or receipt
persistence failure closes the request without allowing the operation.

The default receipt sink refuses allow receipts. Configuring an authorizing
broker without durable receipt storage therefore still fails closed. If a
prompt ends or its revision changes while receipt storage is pending, Urdia
records the stale cancellation and does not release the earlier decision.

Before returning an allow-once response, Urdia persists a redacted receipt.
The receipt contains SHA-256 fingerprints for the provider session, workspace,
tool call, and complete operation; it contains no raw arguments, titles, tool
names, or paths. These fingerprints support audit correlation and cannot be
reused as authorization. Receipt schema version 2 also records whether the
allow came from one-shot review or a trusted session. Trusted receipts keep a
SHA-256 policy fingerprint and generation, never the raw policy identifier.

The CLI installs the deny-all broker unless a run explicitly opts into the
interactive browser flow:

```sh
urdia run \
  --provider qwen \
  --workspace /absolute/path/to/a/repository \
  --prompt "Inspect the project" \
  --permissions-ui \
  --ui-bind 127.0.0.1:0
```

That run creates one `InteractivePermissionBroker` in memory and shares it
between ACP and an authenticated loopback UI. The default bind is
`127.0.0.1:0`; the operating system chooses an ephemeral port. The UI server
stops when the run completes. Without the flag, receipts record reject-once
when the agent offers that choice, or cancellation when no compatible choice
exists.

The permission panel deliberately exposes a bounded projection. Authenticated
`GET /api/permissions` returns only an opaque random handle, the current
revision, operation kind, whether arguments are present, and the number of
locations. It never returns raw arguments, commands, titles, names, paths,
workspace, provider or tool session IDs, or fingerprints. A representative
snapshot has this shape:

```json
{
  "interactive": true,
  "pending": [{
    "id": "<opaque-handle>",
    "revision": 3,
    "kind": "execute",
    "arguments_present": true,
    "location_count": 1
  }]
}
```

The browser resolves a request by posting the exact current revision and one
of the two permitted decisions:

```http
POST /api/permissions/<opaque-handle>/decision
Authorization: Bearer <private-token>
Content-Type: application/json

{"revision":3,"decision":"allow_once"}
```

Because command text, arguments, and paths remain hidden, the panel requires a
second explicit confirmation before sending `allow_once` and tells the operator
to proceed only when the operation is independently expected. `reject_once`
remains the immediate conservative choice.

The matching entry is consumed before its decision is delivered, so duplicate
submissions cannot deliver a second answer. A stale revision is rejected and
an expired or already-resolved handle cannot be reused. ACP independently
checks the Urdia session, provider session, prompt revision, operation, and
one-shot option, then persists the redacted receipt before releasing an allow.
The browser never receives persistent grant choices. If it is absent or
disconnects, the broker and ACP deadline fail closed. Jev, Laya, TypeLLM, and
other AI decision providers do not authorize this operation.

The standalone `urdia ui` command is different: it reads an existing database
and remains a historical, read-only view with no active permission controller.
`urdia chat --permissions-ui` uses the same broker and receipt rules while the
authenticated conversation is active. Without that flag, chat keeps the
deny-all broker.

## Trusted sessions

`urdia run` and `urdia chat` can offer a trusted session only when both
`--permissions-ui` and `--trusted-session` are present. The second flag does
not activate a policy. It makes a host-owned activation flow available in the
authenticated loopback page.

Activation begins from a real pending permission request. Urdia derives the
Urdia session, negotiated provider session, canonical workspace, and current
revision from that request. The browser cannot submit those identifiers. A
prepare request creates a one-use challenge that expires after two minutes.
The browser then shows the public provider label, complete canonical workspace,
all ten explicit `PermissionOperationKind` capabilities, and the fixed
15-minute lifetime before requiring a separate confirmation.

The current capability list is intentionally broad: read, edit, delete, move,
search, execute, think, fetch, switch mode, and other. This is the requested
"permit everything for this session" mode, expressed as a concrete allowlist.
It is not a filesystem or process sandbox. The agent runs as the Urdia user and
an execute operation can reach resources available to that user. Use the mode
only with a provider and workspace you trust.

After confirmation, matching requests in the same Urdia session, provider
session, canonical workspace, and current-or-later prompt revision receive a
host-created trusted allow. ACP still selects only its `allow_once` option.
Before releasing the response, it persists a trusted receipt, acquires the
prompt response gate, rechecks the prompt revision, and asks the broker to
revalidate the exact policy ID, generation, scope, capability, expiry, and
revocation state. A failed revalidation records a cancellation and releases no
permission.

The local page can revoke the active policy at any time. Expiry and revocation
are enforced on authorization and revalidation, independently of browser
polling. The final revalidation is the authorization point: revocation blocks
later authorizations, while an operation that already passed that point may
still start or finish. The policy is memory-only; stopping Urdia or creating a
new broker requires a new activation. Jev, Laya, TypeLLM, other `DecisionProvider`
implementations, and their failures have no activation or authorization API.
The historical `urdia ui` command returns `404` for all trusted-session routes.

The ACP child currently runs as the same operating-system user as Urdia. This
is useful for a local MVP, but it is not a security sandbox. Multi-user hosting
must isolate each worker and avoid mounting general SSH, cloud, or package
registry credentials.

The child also inherits Urdia's process environment so an already configured
provider CLI can authenticate. Start Urdia with a filtered environment when
the parent shell contains credentials unrelated to that provider.

External ACP definitions contain an executable and an argument array. Urdia
does not evaluate a shell string. Arguments should never contain credentials
because operating-system process inspection may expose them.
