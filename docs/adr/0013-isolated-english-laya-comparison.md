# ADR 0013: Isolated English Laya comparison

- Status: accepted
- Date: 2026-09-28
- Supersedes: 0011 for the default Laya adapter target

## Context

ADR 0011 established a hardened local Laya service with the
`typed-decisions` checkpoint. Repeated v2 evaluation was operationally stable
but achieved only 33.3% acceptable-route accuracy in both held-out and
adversarial splits, including unsafe trust, injection, egress, and destructive
choices. That checkpoint was trained for a narrower typed workflow and is a
poor fit for Urdia's general route selection.

Laya publishes an Apache-2.0 `english` checkpoint intended for general English
judgments. Comparing it safely requires preserving the old service, preventing
model alias downloads at runtime, and keeping its credential and listener
independent.

## Decision

Make the Urdia Laya adapter request `english` from a new service on
`127.0.0.1:18766`. Install Laya 0.3.20 and the `convaiinnovations/laya`
snapshot at immutable revision
`55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`; verify `model.safetensors` has
SHA-256 `891102d372688fc2a094dac56a384bc537b87c63f21f9f3dac0be2b7cbc8d86c`.
The runtime loads only that local snapshot, requires the separate root-owned
`laya-english.key` systemd credential, binds loopback, and is denied
non-loopback network access.

Keep the ADR 0011 `typed-decisions` deployment intact on port 18765 for
historical comparison and rollback. Model aliases in evaluation metadata are
marked unpinned because `english` is not itself immutable; the deployment pin,
revision, and hash establish the evaluated artifact.

Retain the adapter's general English routing prompt. Reject the compact safety
prompt tested against v2 because it introduced additional unsafe failures.
Keep both Laya deployments advisory: neither may grant permissions, activate a
trusted session, or trigger automatic execution.

## Consequences

- The English checkpoint improves v2 adversarial acceptable-route accuracy
  from 33.3% to 83.3%, while held-out accuracy remains 33.3%.
- A safety-critical destructive-euphemism failure still blocks automatic use.
- The two services use separate paths, credentials, listeners, caches, and
  systemd units, at the cost of additional disk and memory while both run.
- Laya confidence retains unknown probability semantics and no threshold is
  inferred from Jev.
- The inspected v2 corpus is regression evidence; a future activation decision
  requires a newly frozen blind corpus.

## Validation

- Verify the snapshot revision and model SHA-256 before recording results.
- Verify unauthenticated inference returns HTTP 401 and authenticated responses
  report `routing.model=english`.
- Verify both listeners are numeric loopback addresses and the English service
  cannot reach non-loopback addresses.
- Run each v2 held-out and adversarial case three times, preserving only
  sanitized reports.
- Run workspace formatting, linting, unit tests, and schema/corpus checks.
