# ADR 0011: Isolated local Laya decision service

- Status: accepted
- Date: 2026-09-28

## Context

Urdia needs a local comparison point for the hosted Jev routing adapter. Laya
0.3.20 implements the same `/v1/systemone` request shape, but its stock server
defaults to `0.0.0.0`, authentication is optional, model aliases resolve to
mutable Hugging Face revisions, and it can load checkpoints other than the one
being evaluated. Its Choice confidence also has different semantics from Jev.

## Decision

Add a separate `urdia-decision-laya` adapter. It accepts only a numeric
loopback endpoint, requires a bearer credential, pins the request to
`typed-decisions`, validates `routing.model`, bounds transport data, and labels
the returned probabilities as having unknown semantics. Laya runs remain
advisory and may neither start an agent nor authorize an operation.

Provide an optional systemd deployment for Laya 0.3.20 with CPU-only PyTorch.
The installer downloads `convaiinnovations/laya-typed-decisions` at revision
`1a793eb568e6718f15941d08f85432581df534e3`. The runtime loads that local
snapshot, refuses other checkpoints, requires a distinct `laya_key` systemd
credential, listens only on `127.0.0.1:18765`, and has no non-loopback network
access. It runs as a dynamic user with a read-only installation and systemd
hardening.

Use a Laya-specific v2 fixture whose 27 cases are equal as JSON values to the
Jev v2 cases. Provider metadata remains separate so reports cannot imply that
the Laya weights are the Jev model. Local Laya evaluation does not use the
remote-only `--live` acknowledgement.

## Consequences

- Urdia can compare hosted and local semantic routing over the same examples.
- The Laya credential is independent from the TypeSafe key.
- Startup holds about 1.8 GiB of memory on the current CPU host.
- Package and weight revisions are explicit; updating either requires a code
  and evaluation change.
- Confidence thresholds and automatic routing remain disabled until the local
  results justify a provider-specific policy.

## Validation

- Verify unauthenticated inference returns HTTP 401.
- Verify an authenticated Choice returns `routing.model=typed-decisions`.
- Verify the only listener is `127.0.0.1:18765`.
- Run the held-out and adversarial v2 splits repeatedly and retain only
  sanitized reports.
- Run workspace formatting, linting, unit tests, and fake-server adapter tests.
