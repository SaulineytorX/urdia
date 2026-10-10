# ADR 0016: Host-provided current conversation identity

- Status: accepted
- Date: 2026-10-02

## Context

The browser workspace must identify its current project and agent. Historical
timeline entries can belong to older sessions and cannot supply that identity.

## Decision

The authenticated conversation snapshot accepts an optional host-provided
`context` with `provider_id`, `provider_name`, and `workspace_name`. The CLI uses
the selected registry entry and the canonical workspace's directory name. Parent
paths are omitted. The default hub omits context from JSON for compatibility.
Debug output omits identity, just as it omits conversation content.

This adds no unauthenticated metadata endpoint, stored event field, credential
inspection, routing selection, or authorization authority. Browser rendering
uses DOM text. Existing authentication runs before content access. Tests cover
authenticated visibility, unauthenticated rejection and diagnostic omission.

The visual engine view documents existing CLI adapters; it cannot change or
verify the active engine. Interactive advisory routing and host-side credential
configuration require a separate implementation. ADR 0015 remains in force.

## Execution-profile extension — 2026-10-02

An optional `context.profile` now describes the current launch selected under
ADR 0018: ID/name, agent, inference provider, model, optional effort, selection
source (`explicit_profile` or `default_profile`), and `model_effort_verified`
(currently always false). Direct provider sessions omit this field. The CLI
copies validated host configuration after resolving the selected profile;
browser input cannot supply this identity. Model/effort labels are declarations,
not effective-setting verification. Commands, argv and credentials are omitted.
The extension remains authenticated, memory-only and omitted from snapshot/hub
Debug output. It does not modify stored event formats or the routing gate.
