# ADR 0043: Bounded Python project checks

- Status: accepted, experimental
- Date: 2026-10-06 UTC

## Decision

Add the allowlisted python-unittest-v1 contract to the existing explicit, authenticated
verification flow. Bundle a first-party checker with the binary, reusing the existing
Linux descendant-cleaning supervisor, filtered environment, temporary HOME, deadlines,
cancellation and redacted counts. No browser-supplied command, dependency installation,
AI request, trusted scope or automatic execution is permitted by this contract.

System stdlib unittest discovery runs test*.py with package-based nested discovery.
The supported Python3.12 loader masks excluded directories consistently with source
fingerprinting; unsupported discovery behavior must fail offline compatibility tests.
A passing result requires nonempty tests, no failure/error/skip/expected failure/
unexpected success, consistent exit status and equal bounded source snapshots.
Source-file names and bytes contribute to a single digest; no names, logs or raw
exceptions are projected. See the user guide for exact bounds/exclusions. The host
refuses malformed/contradictory count reports rather than trusting an exit code alone.

Report schema3 is exclusive to this contract: source_file_count1..128, one digest key
python_sources and one project_tests check. Existing calculator schema1/2 and two-file
contracts remain compatible and reject generic-only metadata. Rust validates the
report/contract/schema/exit relationship before accepting helper results. No storage
migration or plan archive authority changes. Summary rows use the existing exact-task
binding and newest-request chronology.

## Limits and validation

Project-authored tests do not supply an independent requirements oracle, and untrusted
code can influence unittest or access other user files/network. This is not a sandbox,
pytest/venv integration, dependency readiness probe or arbitrary-project verification.
Fingerprint equality is a before/after observation of selected sources, not a whole
workspace audit or proof of no transient changes. Diagnostics remain fixed/private.

Offline regressions cover actual generic tax tests, privacy, empty/failing/skipped/
expected-failure/import-error suites, source mutation, consent, deadlines, links and
source-count bounds. Rust checks malformed/contradictory schema/counts and exercises
bundled helpers. Browser acceptance covers generic pass/fail/empty/cancellation,
renewed consent, unchanged host task count and responsive layout. Existing calculator,
permission, plan, recovery and summary acceptance must continue to pass.
