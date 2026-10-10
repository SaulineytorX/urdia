# Dependency audit

This guide is the contributor procedure for checking Urdia's locked Rust
dependency graph. It describes how to reproduce a check; it does not record a
release audit result. A maintainer must review the findings before making any
audit or release claim.

## Scope

The check covers the resolved Rust graph reachable from the workspace,
including:

- all workspace features;
- development dependencies; and
- target-specific dependencies for every target (there is no target filter).

`--all-features` enables workspace features, not every feature of every
transitive dependency. Cargo.lock can retain optional transitive packages
outside this resolved graph. Inventory their declared licenses separately and
repeat the audit whenever features or dependencies change; do not equate the
lockfile package count with cargo-deny's checked graph count.

The policy is in [`deny.toml`](../deny.toml). It uses the RustSec Advisory DB,
allows only the crates.io registry, and denies unknown registry or Git sources.
The advisory ignore list is empty. Multiple versions of an upstream crate are
warnings that require review; they are not silently treated as vulnerabilities.
Wildcard version requirements are denied, including unversioned workspace paths.

The license allowlist is the following SPDX identifiers:
`Apache-2.0`, `MIT`, `ISC`, `BSD-3-Clause`, `Unicode-3.0`,
`CDLA-Permissive-2.0`, and `Zlib`. License detection uses the configured
confidence threshold of `0.8`. A compound SPDX expression must be satisfied
by the allowlist according to its operators.

## Toolchain

Use the Rust toolchain pinned in `rust-toolchain.toml` (currently 1.98.1).
The approved audit tool is cargo-deny 0.20.2, whose release MSRV is 1.88:

```sh
cargo install cargo-deny --version 0.20.2 --locked
cargo deny --version
```

The expected version is `0.20.2`. Do not mix results from another
cargo-deny version with an audit record.

## Run a reproducible check

Run these commands from the repository root. Acquisition may contact crates.io
and the public advisory database; the final command is the offline, locked check
against the snapshot just fetched. Metadata acquisition also downloads the
locked crate sources needed for license inspection on a clean system:

```sh
cargo metadata --locked --all-features --format-version 1 >/dev/null
cargo deny fetch db index
cargo deny --locked --offline check
```

`--locked` and `--offline` are global cargo-deny options, so they go before
`check` in cargo-deny 0.20.2. `--locked` requires the existing `Cargo.lock` to
remain unchanged. `--offline` prevents a check from silently refreshing the
advisory database. If a normal locked check is needed while allowing network
access, use:

```sh
cargo deny --locked check
```

Do not use an unlocked check for audit evidence. If a candidate changes the
dependency graph, inspect its `Cargo.lock` diff and repeat the fetch, snapshot,
and offline check sequence from that candidate checkout.

## Record the evidence

Record these values with the candidate's commit and the UTC date:

```sh
cargo deny --version
rustc --version
sha256sum Cargo.lock
```

The default advisory database is under `$CARGO_HOME/advisory-dbs` (or
`$HOME/.cargo/advisory-dbs` when `CARGO_HOME` is unset). After `cargo deny
fetch db index`, record its Git revision. This directory name is derived from
the configured URL by cargo-deny 0.20.2:

```sh
db_root="${CARGO_HOME:-$HOME/.cargo}/advisory-dbs"
db_dir="$db_root/advisory-db-3157b0e258782691"
git -C "$db_dir" rev-parse HEAD
```

The record should contain the cargo-deny version, Rust version, advisory DB
commit SHA, `Cargo.lock` SHA-256, repository commit, command result, and any
finding identifiers. Keep evidence to those sanitized facts and relevant
crate/version/source names; do not include credentials, tokens, or raw
environment output.

Review every diagnostic before treating a candidate as ready. Resolve an
advisory, source, license, or duplicate-version finding in the dependency or
policy, or document the maintainer decision. Do not add an advisory ignore to
make a candidate appear clean. The command exit code is evidence for the
maintainer's internal report, not by itself a public release assertion.

## Boundaries

This check is limited to the Rust graph. It does not audit the optional Python
services or their package licenses and hashes, downloaded model licenses,
revisions or hashes, or the system SQLite library. Those require separate
review. A passing cargo-deny run cannot support a claim that all Urdia
dependencies, services, models, or host libraries are globally clean.

Cargo.lock registry checksums and the SPDX allowlist provide useful integrity
and policy evidence for this Rust graph. They are not a third-party notice,
complete license-text review, source provenance record, signature, or complete
release attestation. Keep those release reviews separate.
