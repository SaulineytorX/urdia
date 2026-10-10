# Contributing

Use the pinned Rust toolchain and run local checks before proposing a change:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
node --check crates/urdia-ui/assets/live.js
git diff --check
bash scripts/smoke-test.sh --self-test
```

For installation/provider/permission/storage/package changes, run the full offline
`scripts/smoke-test.sh`. It fetches locked crates during its isolated build, then
uses a deterministic fake agent without credentials. Keep evidence outside the
checkout. No hosted CI is included; enabling it requires an explicit owner request.

Keep provider-specific behavior inside its adapter and preserve host permission
checks. Update public guides and applicable ADRs. No decision engine may turn a
recommendation into a permission or automatically activate a trusted session.

Contributions are accepted under Apache-2.0. Report vulnerabilities privately via
the reviewed [security policy](SECURITY.md), never public issues or task messages.
