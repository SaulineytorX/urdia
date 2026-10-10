# ADR 0009: Validated advisory routing

- Status: accepted
- Date: 2026-09-28
- Supersedes: none

## Context

Urdia needs a common route-selection boundary for deterministic rules, hosted
Jev, and future local decision providers. A model response cannot be trusted as
an executable provider ID, and routing must remain separate from permission
authorization. Evaluation also needs stable inputs before any route is used to
start an agent automatically.

## Decision

Add `urdia-routing` between `DecisionProvider` implementations and callers.
The host prepares bounded candidates with case-local opaque IDs and English
descriptions. The resolver validates the request, fingerprints the candidate
set, invokes one decision provider, and validates the selected ID,
probabilities, confidence, and probability semantics. Provider failures,
missing selections, invalid output, and configured low confidence become typed
fallbacks.

Add `urdia-decision-jev` as a TypeSafe adapter pinned to `jev-1.13.0`. It sends
one English Choice question to `/v1/systemone`, reads `TYPESAFE_API_KEY` from
the process environment or a systemd `typesafe_key` credential, bounds response
data, disables redirects, and retries only temporary service responses. It
never stores a raw response or credential. The shared
`__urdia_escalate__` candidate maps to no selection.

Expose `urdia route` as an advisory command. It considers only registered,
currently available agent providers and reports the validated recommendation;
it does not start a provider, open a listener, write an event, activate a
trusted session, or call a permission broker.

Expose `urdia eval-routing` for the versioned English fixture corpus. The
deterministic runner is offline. Jev requires both `--router jev` and `--live`
before fixture data leaves the machine. Reports contain case IDs, selections,
fallbacks, confidence, aggregate semantic metrics, and a separate operational
success rate, without raw responses or API keys.

## Consequences

- Provider-specific network and response handling stays out of the routing
  policy.
- Callers receive a usable route only after host validation.
- Confidence thresholds are optional and belong to a pinned provider/model
  evaluation, rather than a global default.
- Automatic agent execution remains deferred until the corpus establishes an
  acceptable policy.
- The trusted-session lifecycle in ADR 0008 is implemented, but this routing
  path still cannot activate, widen, extend, revoke, or apply it.

## Validation

- Unit tests cover in-set decisions, unknown IDs, malformed probability maps,
  low confidence, explicit escalation, provider errors, redacted errors,
  bounded responses, loopback test endpoints, retries, and timeouts.
- The 15-case fixture runs offline through the same resolver used by the CLI.
- CLI smoke tests confirm routing starts no agent and Jev evaluation refuses to
  run without explicit live opt-in.
