# ADR 0004: Static privacy-filtered timeline

- Status: accepted
- Date: 2026-09-27

## Context

M3 needs a useful visual surface before Urdia has settled live streaming and
interactive permission UX. A local HTTP server would add listener, token,
origin, shutdown, and browser security decisions even for a read-only first
slice. Stored events can also include workspace paths and opt-in conversation
content that should not automatically reach a UI.

## Decision

The first UI is a standalone HTML export produced by `urdia timeline`. Urdia
opens an existing SQLite database with `SQLITE_OPEN_READ_ONLY`, lists recent
sessions, and projects every event through an explicit presentation allowlist.
The renderer omits conversation text, workspace paths, fingerprints, unknown
fields, and detailed errors. It escapes all displayed values.

The document embeds CSS, contains no script or external resource, and starts no
network listener. The CLI creates it with mode `0600` on Unix and refuses to use
the event database itself as the output path.

## Consequences

- The first UI has no inbound port or authentication surface.
- The export is easy to inspect, test, share deliberately, and screenshot.
- It is a snapshot rather than a live conversation view.
- A later live surface can reuse the event projection while adding explicit
  loopback authentication and one-shot permission interaction.
- Exported metadata remains private even though high-risk payload fields are
  omitted.

## Validation

- Tests prove read-only open does not create a missing database or append.
- Renderer tests inject script markup and secret sentinels into omitted fields.
- CLI tests verify atomic owner-only replacement and refuse direct or hardlink
  database overwrite.
- A full fake-agent run generates two sessions while retained content remains
  absent from the resulting HTML.
