# ADR 0044: Reproducible root-crate link observations

- Status: accepted
- Date: 2026-10-06 UTC

## Context

Two independently cloned/build-isolated a1b9da1 candidates produced identical binary
and archive bytes but different sidecar hashes. Inspection found differences only
in root Urdia object disambiguators in the relink map. Canonicalizing those observed
names made every grouped origin/section count and byte size identical. The relink
command includes an isolated map-output path; the stripped deliverable does not
contain those object identities. The failed preparation remains failed evidence.

## Decision

Generate link-section-observations/v2 with explicit
origin_normalization=root-codegen-unit-metadata-v1. Only allowlisted root-object
names under build/target/x86_64-unknown-linux-gnu/release/deps are canonicalized:
Cargo/rustc metadata disambiguators are removed, codegen unit numbers retained, and
bounded generated root objects grouped as urdia.generated.rcgu.o. Preserve all
positive section counts and sizes. Do not normalize dependency/archive identities,
unknown object shapes or substantive binary bytes.

Keep comparison strict: identical source/build identities, complete manifests,
archive/binary bytes and canonical sidecar hash are still required. Sidecars remain
checksum-bound to their binary and included in signature binding. Accept legacy v1
for inspection without implying it has v2 normalization; v2 requires the exact
normalization marker. Do not rewrite previous failed candidates into passing ones.
Run a fresh clean candidate preparation from the new source commit.

## Validation and limits

Regression fixtures vary isolated directories and root disambiguators while keeping
all observations; they must match. Changing codegen unit, section size, dependency
identity or binary bytes must remain different. Existing unknown-input rejection,
private-path/symbol omission and detached binding tests remain. Root-generated
aggregation is a diagnostic normalization, not source-level license attribution.
All package/publication/owner gates remain unchanged.
