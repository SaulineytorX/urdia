# ADR 0046: Project launcher executable selection

Status: accepted

## Context

A source-preview user rebuilds the current checkout and starts its project helper.
An older installed executable on PATH must not silently override that build.
The deterministic reference agent used by the demo must follow the same rule.

## Decision

Select an explicit executable path first, then the existing local release build.
Look up PATH only when the local build is absent. Resolve and validate the chosen
path; an existing invalid build or broken symlink fails instead of choosing another
version. Explicit overrides remain available. The helper does not install files,
change PATH, update packages, inspect credentials or start a session during --check.
Permission, loopback, state privacy and advisory boundaries stay unchanged.

## Verification

Ten offline launcher regressions include a copied source helper without Git,
local executable links plus deliberately failing installed stubs, readiness with
no state writes, two real authenticated HTTP turns, unauthorized refusal and
unchanged project/database privacy. Validate the next actual source archive
separately; existing candidate identities remain immutable.
