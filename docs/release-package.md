# First core package content plan

The [machine-readable plan](../deploy/release/core-linux-x86_64.json) fixes the
first candidate's content. The private candidate generator now implements this plan; no public artifact or
release approval is implied.

| Item | Content |
| --- | --- |
| Target | `x86_64-unknown-linux-gnu`, locked default features, `urdia` binary |
| Archive | `urdia-0.1.0-x86_64-unknown-linux-gnu.tar.gz` with one same-named root |
| Executable | `bin/urdia`, mode 0755 |
| Instructions | Standalone [package README](package-readme.md), copied to archive root |
| Urdia terms | LICENSE and NOTICE, mode 0644 |
| Third-party terms | [Core notice index](core-third-party-notices.md), filtered provenance JSON and every mapped original text |
| Validation agent | Built separately from the same commit; not shipped to users |

The plan's 205 registry entries come from the exact Linux normal/build closure:

```sh
cargo tree --locked -p urdia --target x86_64-unknown-linux-gnu \
  --edges normal,build --prefix none --format '{p}'
```

This conservative notice scope includes build dependencies, not just proven linked
code. Development-only and inactive other-target packages are excluded unless
reachable in this closure. The first binary package includes no Python wheels,
models/cards, external agents, deployment scripts/units, internal notes, evaluation
reports, secrets, configuration, databases, or host shared libraries. A source
archive and optional deployment bundles need separate plans; this does not prevent
users from choosing another agent or decision provider.

System SQLite remains an external dependency: no `bundled` rusqlite feature is
selected. Final ELF inspection must identify SQLite, libc, and compiler runtime
requirements and establish the supported ABI. The plan does not promise universal
Linux portability or a static executable.

## License gaps resolved in the inventory

- r-efi 6.0.0: the hash-verified crate AUTHORS contains the MIT grant and
  copyright attribution; MIT is selected for its inventory resolution.
- rsqlite-vfs 0.1.1: all four shipped Rust sources/original-manifest files match
  Git blobs at its embedded revision in the SQLite workspace. The inherited MIT
  declaration and root LICENSE were retained with provenance; repository identity
  is established by content matches rather than guessed from a similar name.
- tokenizers 0.23.2: the exact-version PyPI sdist SHA-256 and size were checked;
  its Apache license and PKG-INFO were retained. This supplies a same-version
  license, not attestation that the selected wheel was built from those bytes.

These three packages are outside the first core closure. Their discovery gaps
are resolved without inferring approval for distributing optional wheels or all
source targets. The core ledger records 205 candidate license choices, retaining compound AND
obligations rather than selecting only one component. Ring's original leading
source comments (277 references, including notices after assembler directives) and fiat AUTHORS are retained. Its crate VCS
metadata is dirty; only the ten shipped fiat files were individually matched to
Git blobs at the recorded revision before recovering AUTHORS. This does not
attest the entire crate's source revision. Rust's installed library notice and
all 12 supplied license terms are included separately from Cargo dependencies;
that broad notice is not a claim that every named component is linked.
See the native notice review (private review material omitted from this snapshot).
Final inclusion and redistribution-obligation approval remains open.
Unicode data terms and webpki-roots' CDLA text remain part of the selected notices.

## Assembly and remaining checks

The generator builds the explicit target and bin, verifies the plan's
input hashes against the candidate checkout, and compares the dependency closure
with the pinned registry list. It generates the filtered inventory, copies exactly
the listed files, and rewrites only the core index's `../third-party/` link prefix
when placing it at archive root. Original text bytes remain unchanged. It rejects
unexpected files, symlinks, stale mappings, missing texts, and hash mismatches.

Before final packaging, review the applicable upstream texts and native headers,
record license choices, and resolve notice obligations for this scope. Build and
inspect the actual ELF, validate archive contents and paths, then exercise an
installed copy with the separate fake agent. Test upgrade/backup/rollback using
disposable data, record checksums/provenance, and add signing. Publication requires
owner authorization; the current plan explicitly records `publication_allowed: false`.

## Build a private candidate

Prerequisites: the source-build dependencies, Linux Python 3 with the existing
process supervisor's `/proc`/pidfd APIs, and `readelf` (binutils). Cargo may fetch
public crates during the build. No model inference or AI credentials are needed.
The generator uses a new temporary Cargo home and a filtered build environment;
it preserves only the selected Rust toolchain location. It bounds commands with
the existing descendant-cleaning supervisor and refuses source changes during
compilation. Keep this a single-owner checkout while it runs.

```sh
mkdir -p "$HOME/urdia-candidates"
chmod 700 "$HOME/urdia-candidates"
python3 -I -B scripts/package-core.py \
  --output-dir "$HOME/urdia-candidates/core-candidate"
```

The output directory must be new and outside the checkout. A dirty checkout is
refused unless `--allow-dirty` is explicitly supplied for development. A successful
run writes the tar.gz, SHA256SUMS, and sanitized package-evidence.json into a private
directory. The evidence records the commit, dirty flag, checkout fingerprint,
file hashes/modes, ELF libraries, and glibc symbol versions. It never marks the
candidate approved for publication. Checksums detect corruption, not authenticity;
signing/provenance remains a separate gate.

Archive ordering, owners, modes, tar timestamps (commit epoch), and gzip metadata
are deterministic for fixed file bytes. The build remaps Rust/C source and temporary build paths, strips release symbols,
and sets SOURCE_DATE_EPOCH to the commit epoch. Independent binary rebuild
comparison must still be performed for each candidate. Path remapping is supported
by [rustc](https://doc.rust-lang.org/rustc/command-line-arguments.html#--remap-path-prefix-remap-source-names-in-output);
stripping uses [Cargo release profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#strip). Two clean builds of checkpoint e1e9d14 produced identical binary and archive
bytes on the same Ubuntu 24.04 host; see the
comparison evidence (private review material omitted from this snapshot). This does not
establish different-host reproducibility. Candidate checks do not publish anything
or create a tag.
The earlier development candidate contained 146 files; the expanded plan has
299 files. The clean e1e9d14 candidate requires glibc symbols
through 2.39 plus dynamic system SQLite; this is an observed ABI floor, not a
portability matrix for every distribution or future build host.

## Rehearse installation and recovery

Build the separate offline reference agent and run the package rehearsal:

```sh
cargo build --locked -p urdia-acp --bin urdia-fake-agent
python3 -I -B scripts/test-core-package.py \
  --candidate-dir "$HOME/urdia-candidates/core-candidate" \
  --fake-agent "$PWD/target/debug/urdia-fake-agent" \
  --evidence "$HOME/urdia-candidates/core-candidate/packaged-rehearsal.json"
```

The harness checks the recorded archive/file hashes, exact paths, regular-file
kinds and modes before copying into a private temporary prefix. It runs the
installed executable with only the explicit fake agent and a filtered environment.
It checks event creation, default content privacy, a quiescent database/sidecar
backup, whole-prefix replacement, installation rollback, exact database restore,
restored history export and another session, then uninstall without deleting data.
Every command is bounded; temporary runtime/data directories are removed.

This rehearses replacement using the same 0.1.0 candidate. It does not certify
cross-version or future-schema upgrade/downgrade compatibility. The evidence is
sanitized; the source commit and dirty flag remain visible. The evidence output
must be a new file outside the checkout. No live installation, service, model,
credential, or non-loopback listener is used.

## Detached candidate signature

`scripts/attest-core.py` requires Linux's `/usr/bin/openssl` with Ed25519 support
([OpenSSL raw-input signing](https://docs.openssl.org/3.0/man1/openssl-pkeyutl/)).
It signs a canonical attestation binding the archive, SHA256SUMS, package evidence,
clean source commit, and source fingerprint. It first validates the archive and
refuses dirty candidates, duplicate JSON keys, and existing attestation outputs.
The private key must be a regular owner-only Ed25519 PEM file; the tool never
copies it into the candidate. This noninteractive version does not support
passphrase entry; protected-key or hardware signing needs a separate integration.

```sh
python3 -I -B scripts/attest-core.py sign \
  --candidate-dir "$HOME/urdia-candidates/core-candidate" \
  --private-key /absolute/private/location/release-ed25519.pem
python3 -I -B scripts/attest-core.py verify \
  --candidate-dir "$HOME/urdia-candidates/core-candidate" \
  --public-key /absolute/trusted/location/release-ed25519.pub.pem
```

Verification must precede installing or executing a received candidate. Obtain the
public key through an independently trusted channel; a public key delivered only
inside the same untrusted download does not establish publisher identity. The
recorded SPKI SHA-256 identifies the supplied key and does not establish trust by
itself. Never repurpose the Git SSH key, a provider credential, or an ephemeral
test key as the project's release identity. No production release key has been
created by these changes.

The attestation is a local candidate binding, not a complete SLSA provenance claim,
hardware key integration, or publication approval. `publication_allowed` remains
false. The project still needs a reviewed release-key custody, trust distribution,
and revocation policy before signed public releases.

## Repeat the independent comparison

Use two clean clones of the exact candidate commit at different paths. Run the
generator separately in each clone with new output directories; each invocation
creates its own Cargo cache and target tree. Do not pass `--allow-dirty` or share
a build tree. Require matching source commit, source fingerprint, plan hash,
compiler identities, file manifests (including binary hash), and complete archive
hashes before recording a pass. Check both candidates with the installed rehearsal.

The generator's own evidence conservatively says an independent rebuild is not
verified: one invocation cannot establish that claim. Retain the separate
comparison report alongside both build records. The offline comparator validates
each real archive before comparing source/plan/toolchain identities, file manifests,
file bytes, and complete archive hashes:

```sh
python3 -I -B scripts/compare-core-packages.py \
  --first "$HOME/urdia-candidates/core-first" \
  --second "$HOME/urdia-candidates/core-second" \
  --evidence "$HOME/urdia-candidates/core-first/comparison.json"
```

The evidence path must be new and outside the checkout. The tool writes a private
complete report atomically, never replacing an existing report. It refuses dirty
candidates, malformed archives, mismatched build metadata, differing content, and
a directory compared with itself. It executes no candidate binary and needs no
credential, signing key, model, or listener. Keep the candidate directories stable
while it runs. Matching directories alone cannot prove separate builds: copied
candidates would also match. Build independence and host coverage must be documented
with the two actual build records; the report explicitly does not verify those
claims or publisher identity. Use detached signature verification separately. A future candidate, toolchain,
platform, or plan change needs fresh evidence. Compare on another supported host
before claiming reproducibility across build environments.

## Inspect a receiving host

Use the [host compatibility guide](core-host-compatibility.md) before executing a
received candidate. The checker validates actual ELF requirements and observes
the receiving host without launching the binary. A passing preflight is separate
from signature trust and installed runtime evidence. The existing rehearsal is
unchanged and does not enforce these checks automatically.

## Verify license choices and notice coverage

Run the [core license review](core-license-review.md) before preparing a candidate.
The generator enforces the current bounded choice policy and per-component
notice references, including ring native and Rust library sections. Unknown
declarations or changed selections require review. Mapped text/choice coverage
does not establish exhaustive native applicability or distribution approval.

New private core builds record [linker section observations](core-link-observations.md) bound to
the executable and package evidence; source attribution and redistribution review
remain separate release gates.


Root-crate relink object metadata is canonicalized in link-observations schema v2
under [ADR 0044](adr/0044-reproducible-root-link-observations.md). Unit numbers,
section counts/sizes and dependency identities remain; archive/binary bytes and
sidecar bindings are still compared exactly. Legacy v1 remains inspectable without
implying canonical rebuild evidence. These diagnostics are not license attribution.
