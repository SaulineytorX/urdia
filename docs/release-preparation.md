# Private candidate preparation and release boundary

Candidate preparation runs locally. GitHub-hosted CI and manual preparation are
removed from the active workflow directory by owner request to avoid new runner
usage. The archived manual definition (private review material omitted from this snapshot)
is reference material only; do not restore or dispatch it without a new explicit
owner request. Local preparation does not publish an archive, create a tag/release,
change visibility, establish production signing identity, or authorize
redistribution. The core plan remains provisional.

## Local preparation

Use a clean checkout with the documented Linux source-build prerequisites,
Python 3 with the existing `/proc`/pidfd supervisor support, OpenSSL for regression
tests, and readelf. Review and pass the repository's required source checks first.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
node --check crates/urdia-ui/assets/live.js
git diff --check

mkdir -p "$HOME/urdia-candidates"
chmod 700 "$HOME/urdia-candidates"
python3 -I -B scripts/prepare-core-candidate.py \
  --expected-commit "$(git rev-parse HEAD)" \
  --output-dir "$HOME/urdia-candidates/reviewed-candidate"
```

The expected commit must be a full lowercase 40-character hash. The coordinator
refuses a dirty checkout, another HEAD, an existing output, or output inside the
checkout. Keep the checkout unchanged throughout preparation.

It creates two separate disposable Git clones, checks out the expected commit,
and runs the core generator independently with separate empty Cargo caches and
build trees. The comparator validates both actual archives. A third source build
runs the 21-check installed smoke and supplies the same-commit fake agent. Both
packaged copies then pass the ten-check installation/backup/replacement/restore
rehearsal. Final evidence must match the expected commit, source fingerprint,
archive hashes, clean-source flags, and successful phase results.

All subprocesses use a filtered environment and the existing descendant-cleaning
supervisor. Git clones are local and do not use SSH credentials; Cargo can fetch
public locked crates. There are no models, provider credentials, production keys,
or listeners. The normal successful path removes disposable sources/builds before
publishing the final preparation report. Output remains private and contains:

- `first/` and `second/`: archives, checksums, package evidence, and recovery reports;
- `comparison.json`: actual archive/identity comparison;
- `smoke/smoke-evidence.json`: clean source smoke;
- `preparation-evidence.json`: source identity, phase outcomes/durations, result,
  and explicit no-publication/no-production-signature flags.

Failure after preflight preserves a private failure report with the phase name
and completed phases, not raw errors, paths, arguments, or environment values.
Partial artifacts are development evidence and must not be treated as a completed
candidate. Preflight failures create no output. Interrupted validation is a failure;
restart with a new output directory. No existing output is overwritten.

## GitHub-hosted validation disabled

Both workflow definitions are stored outside `.github/workflows`, in
the archive (private review material omitted from this snapshot). New commits on `main`
contain no active workflow definitions. Use the local checks and coordinator
above for candidate evidence; no hosted CI result is required or implied.

Removing workflow files does not cancel runs already queued or running, remove
historical charges, or change repository-wide Actions settings. Git access alone
cannot verify those settings. For an additional repository-wide stop, the owner
can select **Settings → Actions → General → Disable actions → Save** and cancel
any queued/running jobs in Actions. See GitHub's
[Actions settings guide](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/enabling-features-for-your-repository/managing-github-actions-settings-for-a-repository).

## Production signing identity and publication

The existing [detached signer](release-package.md#detached-candidate-signature)
verifies an independently supplied Ed25519 public key. Ephemeral-key test evidence
is not a production identity. Keep these owner-reviewed decisions separate:

- custody and backup of the release private key, signing-machine access, and
  the protected-key/hardware integration if required;
- independently trusted public-key distribution and pinned SPKI fingerprint;
- rotation/revocation, affected-version notices, and recovery from lost keys;
- the exact candidate commit, approved notice/platform/security review, and
  final archives/checksums/attestations selected for publication.

No production key is generated or installed by the coordinator/workflow. The
current signer does not support interactive passphrase entry or hardware keys.
Do not repurpose Git SSH keys or provider credentials. Do not add a release key
to a workflow as an informal shortcut around the reviewed custody/trust decision.

Before any public action, finish the pre-release checklist, review the exact
artifacts and signing identity with the owner, and obtain explicit approval for
that publication/visibility/tag/deployment action. Private preparation evidence
cannot satisfy those approvals by itself. Different-host portability and
reproducibility, final redistribution obligations, public-source sanitization,
security contact, and distinct-version/schema recovery remain separate gates.
