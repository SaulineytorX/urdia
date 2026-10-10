# ADR 0045: Review-only source projection

Status: accepted for private preparation, not publication

## Context

The development repository contains internal host records and archived workflows.
A clean binary candidate does not establish that its repository history is suitable
for public distribution. Useful source, tests and third-party notices must remain
available without copying operational records or activating hosted jobs.

## Decision

Prepare a new source snapshot from an exact clean commit with a tracked-file
allowlist. Exclude internal documents, all GitHub metadata, Git history and the
private projection tool/templates. Preserve selected regular source files and
executable modes. Replace five entry documents with reviewed preview templates;
remove links to excluded tracked material and explicit private acceptance paths
only from Markdown. Missing public links, selected symlinks and narrow sensitive
pattern matches refuse preparation before delivery. Do not silently sanitize code.

Archive order, timestamps and gzip metadata are deterministic. Evidence binds each
file and archive by SHA256 and keeps publication approval false. An optional
tracked owner contact record is validated against both reporting policies;
contact confirmation means owner designation, not verified email delivery. This is
not exhaustive secret detection, a history audit, signing or permission to publish.

A future public repository may start from an independently reviewed snapshot;
choosing that repository and its initial history requires an owner decision.
Never rewrite the existing private history as part of preparation. Keep hosted CI
absent under ADR 0040. Dedicated security contact and exact publication approval
remain prerequisites. Existing binary candidate identity is unchanged.

## Validation

Offline fixture tests cover privacy exclusions, preserved code/licenses/modes,
deterministic output, dirty/wrong source rejection, secret-safe diagnostics,
missing public links, symlinks, output refusal and interrupted delivery cleanup.
Validate actual projected source separately before treating it as review-ready.

The two optional Python artifact-evidence JSON records now live beside their
deployment locks in deploy/laya and deploy/typellm. Their bytes and hashes are
unchanged; installer regression tests use these public paths. Internal operational
prose remains excluded.

The allowlist also includes the versioned evaluation corpora/results and public
configuration examples. These are required by routing tests and offline smoke;
excluding them is a validation failure even when release compilation succeeds.
