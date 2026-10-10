# Local timeline

`urdia timeline` creates a standalone read-only snapshot of recent Urdia
sessions:

```sh
urdia timeline \
  --database .urdia/events.sqlite \
  --output .urdia/timeline.html \
  --limit 50
```

The database must already exist. Urdia opens it with SQLite's read-only flag,
does not start an agent, and does not discover provider credentials. The output
contains its styles inline, has no JavaScript or network resources, and does
not open a listening port. On Unix, Urdia sets its mode to `0600` even when
replacing an existing file.

For a live local view, start the opt-in loopback server:

```sh
urdia ui \
  --database .urdia/events.sqlite \
  --bind 127.0.0.1:0 \
  --limit 50
```

The default bind is `127.0.0.1:0`: the code accepts loopback addresses only,
and port `0` asks the operating system for an ephemeral port. The command
prints one private URL containing a random in-memory token. Open that complete
URL in a local browser. The browser keeps the token in session storage, removes
it from the address bar, and sends it as a Bearer token when polling
`GET /api/timeline` every two seconds. Stop the process with `Ctrl-C`.

The live shell and static assets do not require the token, but they contain no
history. The authenticated endpoint returns rendered HTML from the same
privacy projection used by the export. It has no write or permission actions,
does not enable CORS, and applies no-store, CSP, referrer, MIME-sniffing, and
frame protections. Treat the printed URL as private; this is a local single
user convenience and does not provide multi-user isolation.

## Interactive one-shot permissions

`urdia ui` is intentionally historical and read-only. To approve or reject a
permission request while an agent is running, opt into the transient panel on
that run:

```sh
urdia run \
  --provider qwen \
  --workspace /absolute/path/to/a/repository \
  --prompt "Inspect the project" \
  --permissions-ui \
  --ui-bind 127.0.0.1:0
```

This mode starts the same loopback-only server inside the run and shares one
in-memory broker with ACP. The browser receives an authenticated list of
pending requests containing only an opaque handle, revision, operation kind,
argument-presence flag, and location count. It can submit exactly one
`allow_once` or `reject_once` decision for the current revision. Raw arguments,
commands, titles, names, paths, workspace, session and tool IDs, and
fingerprints remain in the Urdia process.

ACP validates the request again and persists a redacted receipt before an
allow is released. Duplicate or stale decisions are rejected. A missing or
disconnected browser reaches the normal broker/ACP timeout and cannot create
an allow. Persistent grants are never offered, AI decision providers do not
participate in authorization, and the server stops when the run finishes.
Treat the printed URL as private. The panel is a local convenience and does
not provide sandboxing or multi-user isolation.

## Streaming conversation

`urdia chat` starts a provider and the authenticated loopback surface together:

```sh
urdia chat \
  --provider qwen \
  --workspace /absolute/path/to/a/repository \
  --permissions-ui \
  --bind 127.0.0.1:0
```

The browser submits one active prompt at a time and polls a bounded in-memory
snapshot while provider-neutral text fragments arrive. A client-generated
turn ID makes retries idempotent. The page can cancel the active turn and, when
the flag is present, answer the same one-shot permission requests described
above. Prompt and response text is visible through the authenticated endpoint
but remains absent from SQLite unless `--persist-content` is selected.

The conversation displays at most 64 turns, retains up to 1,024 accepted turn
IDs to prevent replay within the session, limits prompt and displayed response
sizes, and applies backpressure through a bounded host command channel. Start
a new chat after reaching that session limit. Stop it with `Ctrl-C`. The
printed URL is private because it grants access to the live in-memory
conversation.

## Trusted session control

`urdia run` and `urdia chat` may add `--trusted-session`, but only together
with `--permissions-ui`. This flag offers a separate prepare/confirm panel; it
does not activate access by itself. Activation starts from a real pending
request and displays the current Urdia session, provider, canonical workspace,
all ten operation capabilities, and the 15-minute lifetime.

The capability list includes execute and destructive operations. This mode is
not a sandbox: the provider can reach anything available to the Urdia
operating-system user. The policy stays in memory, is scoped to the current
session/provider session/workspace and revision floor, expires automatically,
and can be revoked before later authorizations. ACP still receives only
one-shot responses, with a redacted receipt persisted and the policy
revalidated before release. See [permissions](permissions.md) for the complete
boundary.

The timeline shows:

- provider name and ID;
- declared authentication and inference location;
- session status and event count;
- lifecycle events, timestamps, and store sequence numbers;
- redacted permission kind, decision, revision, and location count.

It deliberately hides:

- full workspace paths;
- prompt and response text, including content retained with
  `--persist-content`;
- permission fingerprints and raw arguments;
- unknown event fields and provider-specific payloads;
- detailed error payloads.

The generated file still reveals that sessions occurred and which providers
were used. Keep it private and delete exports that are no longer needed.

## Workspace navigation

The live shell separates Conversation, Activity history, and Decision engines.
A history-only process opens Activity history automatically. See the
[workspace guide](workspace-ui.md) for task states, engine setup limits and
offline browser acceptance. Session badges describe whether the session was
closed, independently of individual task success; earlier errors stay visible.
