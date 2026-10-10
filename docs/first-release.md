# First preview release scope

The proposed first version is a local, single-user Linux/WSL source preview.
There is no published release/tag or public installer yet. This document defines
what to finish and review; it does not authorize publication or change repository
visibility. The target version/name is provisional until the candidate is reviewed.

## One clear first-run path

Use the [installation guide](installation.md), then the
[WSL/project launcher](windows-preview.md). Start with the offline demo to confirm
that the browser works. The demo is labeled **not AI**. Configure an available
coding agent and its account in the setup guide, choose the project, and send one
concrete task. Review each tool permission, inspect the result, then explicitly run
an applicable check. For multi-step work, review a plan and each step's result.

The browser should make these next actions clear: setup, task submission, permission
review, result inspection, checks and recovery. No typed CLI command is presented as
an installed command when the user is still running from source. A [terminal source-setup assistant](guided-setup.md) now guides local builds and
startup. The native Windows installer is deferred to v2; Linux/WSL is the v1 path.

## Included and measured

- Local loopback workspace, token authentication, streaming, cancellation and
  explicit one-shot permissions; Urdia is not a security sandbox.
- Configured coding agents and reviewed model/effort profiles, explicit per-step
  selection and optional Jev/Laya/TypeLLM advice. Default sending remains manual; explicit [automatic Jev preview](automatic-jev.md)
  can dispatch one submitted task under ADR0050, with the production gate unpassed; account availability is separate from executable discovery.
- Reviewed plans, saved-outline recovery, failed original-agent reconnection and
  read-only results with measured host times. No invented token/cost values.
- Explicit [Python unittest project checks](python-project-checks.md) and two fixed
  calculator contracts. Other frameworks need agent-owned tests/review until a
  corresponding host contract is implemented and evaluated.

Ubuntu24.04 x86_64 is the validated host baseline. Windows testing uses WSL; native
Windows, other-host binary compatibility and a public signed installer are not
implied. Each advertised adapter retains its documented version and smoke limits.

## Before publication

Use the pre-release checklist (private review material omitted from this snapshot) against the exact
clean candidate commit. Preserve local checks: GitHub-hosted workflows stay disabled.
Do not turn an old unchecked template item into a claimed pass without evidence.

Remaining release decisions/evidence include final distribution/source sanitization,
final notices and dependency audit for that candidate, security reporting contact,
installation/backup/rollback and platform statements for the delivered form, and
owner review of the exact source/package and visibility/publication action. If
publishing binaries, production signing identity/trust distribution is additional;
private temporary signing tests are not a production identity.

[Private candidate preparation](release-preparation.md) already exists. It prepares
reviewable evidence and does not publish. Finish that concrete review before asking
for final publication approval. This preview must not promise autonomous multi-step
orchestration or support for every model/framework/platform.


The private 8449d9b candidate report (private review material omitted from this snapshot)
records matching independent builds and actual packaged-install/browser/recovery
evidence. It remains a review candidate; a passing build does not authorize publication.

Security reports go to [saulineytor@mpcodersmx.com](mailto:saulineytor@mpcodersmx.com),
as designated by the owner. See [the policy](../SECURITY.md). Selecting a contact
does not approve publication, repository changes or production signing.

The workspace now has [Jev key configuration](jev-credentials.md) with optional
OS-vault remembering. Native Windows-vault acceptance is pending. Linux GNOME
acceptance now passes in an isolated real vault; ordinary server mode remains
session-only when no system vault is installed. The earlier e8a7425 source and 8449d9b core
candidates predate this runtime change. The refreshed source review and its exact
evidence scope are recorded in the private acceptance report (private review material omitted from this snapshot);
public delivery remains a separate owner decision.

Static [frontend verification](frontend-checks.md) now covers desktop/mobile smoke
and a fixed counter contract. Build/server-based frameworks remain outside scope.
