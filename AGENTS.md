# Instructions for coding agents

Read docs/architecture.md and relevant provider/permission guides before changing
behavior. Keep agent execution, judgments, permissions and persistence separate.
Decision engines never grant permissions. Preserve deny-by-default, one-shot ACP
decisions and durable redacted receipts. Use loopback only; never copy credentials
or commit runtime state. Urdia is not a security sandbox.

Before committing:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
node --check crates/urdia-ui/assets/live.js
git diff --check
```

Update user documentation when behavior changes. No GitHub workflows are included;
do not activate hosted CI without an explicit owner request. Do not publish, change
visibility or rewrite Git history without explicit owner authorization. The source
preview offers an explicit experimental per-task Jev mode; manual remains default.
Preserve exact selectors, no-match refusal, idempotent delivery and separate tool
permissions. This does not imply autonomous plan loops, production routing quality
or universal platform/model support. See docs/automatic-jev.md and ADR0050/0051.
