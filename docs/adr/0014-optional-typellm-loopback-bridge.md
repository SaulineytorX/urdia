# ADR 0014: Optional TypeLLM loopback bridge

- Status: accepted
- Date: 2026-09-29

## Context

Urdia needs a provider-neutral way to compare Jev and Laya with typed decisions
from open autoregressive models. TypeLLM applies JSON Schema constrained
decoding to models served by SGLang. It is a Python client library rather than
a System One compatible server, so the existing Jev/Laya wire contract cannot
be reused without bypassing TypeLLM's schema compilation and token mapping.

The current Urdia host has no supported GPU, SGLang runtime, or local model
weights. Its installed Qwen Code CLI uses hosted inference and is not an
SGLang-served checkpoint. A live TypeLLM evaluation on this machine would
therefore misrepresent the available runtime.

## Decision

Add `urdia-decision-typellm` behind the existing `RouteResolver`. It talks only
to an authenticated numeric-loopback bridge, validates the configured model
and complete probability set, limits input to 23 host candidates plus Urdia's
reserved escalation outcome, and returns
`ConstrainedTokenDistribution` semantics with no confidence value.

Provide an optional Python bridge that converts Urdia's bounded route request
into one TypeLLM enum question with probabilities, temperature zero, no
requested thinking, and no permutation averaging. Pin TypeLLM 0.2.6 and verify
its wheel
SHA-256 `0bb5f6be57a8c041edfceda7c223d7e5fc6c84d5890499d54fcc2ccb63c1f325`.
The bridge binds `127.0.0.1:18767`, requires its own systemd credential, and may
connect only to a separately operated numeric-loopback SGLang endpoint. It also
requires an existing absolute tokenizer directory and disables runtime model
downloads.

Keep installation and activation opt-in. Do not install or start TypeLLM or
SGLang on the current host. A real evaluation fixture and report require an
actual revision-pinned model and runtime; no hypothetical model result will be
checked in.

TypeLLM remains advisory. It cannot start an agent, activate a trusted session,
or grant a permission. A configured confidence threshold fails closed because
the adapter does not manufacture a calibrated confidence value from the
constrained distribution.

## Consequences

- Urdia gains a stable adapter boundary for open models without depending on
  Python or SGLang in the main Rust process.
- Any SGLang-compatible model can be selected explicitly, but comparisons must
  pin and record the exact model revision and runtime configuration.
- Candidate sets larger than 23 fail closed rather than being truncated.
- The bridge and SGLang consume separate processes and must both remain private
  loopback services.
- Live quality, latency, stability, and memory use remain unmeasured until a
  suitable model-serving host is available.

## Validation

- Test the Rust adapter against a fake authenticated bridge, including malformed
  probabilities, wrong models, candidate limits, escalation, and secret-safe
  diagnostics.
- Test the Python bridge with a fake TypeLLM client and inspect the exact enum,
  English instructions, and sanitized response.
- Validate the systemd unit, shell scripts, Python syntax, workspace tests, and
  linting without activating the service.
- Before a live evaluation, verify both listeners are loopback-only and freeze a
  fixture with the exact model revision.
