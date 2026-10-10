# ADR 0034: Operator project plans in browser memory

- Status: Extended by [ADR 0035](0035-host-session-project-plans.md); the tab-only
  storage decision below is historical.
- Date: 2026-10-03 UTC

## Context

The owner wants a project objective broken into tasks and eventually executed by
different suitable agents/models. Existing explicit profile selection, advisory
engines and independently counted calculator checks provide task-level building
blocks. Automatic dispatch remains blocked by ADR 0015's evaluation gate.

## Decision

Introduce an operator-authored outline in Conversation with 1–8 ordered steps,
reviewed scope and acceptance criteria. Editing/reviewing/preparing drafts is
local and inert. Each unchanged prepared draft binds to an observed accepted UUID
through the existing prompt admission path. No new host API or permission authority.
The existing selector controls the agent for each explicitly sent task; the plan
neither switches agents nor grants scope. Prompt scopes are instructions, not a
security policy or executable verifier.

A completed turn requires separate operator review before the next draft is offered.
Label that review distinctly from independent verification. Failed/cancelled tasks
can be explicitly retried. Bind actual host observations, never inferred agent
claims. Keep uncertain planned delivery reserved until accepted observation or
explicit rejection, and reuse the original UUID on retry. A changed prepared draft
is unlinked, avoiding incorrect assignment based on approximate text matching.

Use tab memory only and expose that loss-on-refresh boundary in the panel and guide.
Do not persist the outline as separate plan state in localStorage, SQLite or source
files. Sent task prompts follow the existing explicit content-retention settings;
agent processes may keep their own records. Clearing an
idle plan preserves host tasks and composer text. Keep bounded fields, text-only
DOM rendering, stable idle nodes and responsive layout.

## Consequences

This provides a reviewable sequential task outline, not autonomous orchestration,
an AI-generated plan, parallel execution or independently validated generic tasks.
Browser review/ordering is a workflow aid, not host-enforced authorization; callers
of the normal prompt API are not bound to it. No plan storage/handoff promises.
Future host-owned plans require authenticated revision/admission semantics and
explicit content-retention choices; AI planning and routing need representative
quality evaluations before activation. These remain separate slices.
