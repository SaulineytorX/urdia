# Core license choices and notice obligations

Scope: the planned private Linux x86_64 core binary, 205 locked normal/build
registry packages, with 334 distinct retained texts, 277 ring source-comment references
and 13 Rust library/toolchain notice references. Build-time dependencies are
included conservatively; inclusion in this graph does not prove binary linkage.
The package contains Urdia's Apache-2.0 LICENSE/NOTICE, a per-component notice
index, original texts, provenance inventory and candidate choice ledger.

## Repeat the technical coverage check

```sh
python3 -I -B scripts/review-core-licenses.py \
  --evidence "$HOME/urdia-candidates/core-license-coverage.json"
```

Use a new evidence path outside the checkout, with an existing parent directory.
The offline check writes a complete private report atomically and never replaces
an earlier report. It records hashes of the plan, lock, inventory, choices,
notice index, LICENSE and NOTICE, and a row for every component with its exact
crate hash, declared/selected expression and retained candidate paths/hashes.
It rechecks inputs before producing success. It does not build, execute a model,
read credentials, change services, or authorize publication.

To additionally compare the inventory's original file bytes with all 205
hash-verified crate archives already in a local Cargo cache, use:

```sh
python3 -I -B scripts/review-core-licenses.py \
  --crate-cache "$HOME/.cargo/registry/cache" \
  --evidence "$HOME/urdia-candidates/core-license-archive-review.json"
```

This reads `.crate` archives only, without using Cargo credentials/configuration
or downloading missing dependencies. Every selected artifact must match its
recorded hash. Missing archives, original-file byte mismatches, and named
LICENSE/LICENCE/COPYING/COPYRIGHT/AUTHORS/NOTICE files outside the inventory are
fatal. The report records per-archive checks and named NOTICE paths. This filename heuristic alone does not detect other filenames or inline notices.
For ring, the archive check additionally validates all retained source-comment
byte ranges against the original source-file hashes, and requires coverage of
defined copyright/license/permission signals across `.rs`, `.c`, `.h`, `.S`,
`.pl`, `.py`, `.asm` and `.inc` files. It catches notices after blank lines and
assembler directives; it is not an exhaustive legal/source parser.
Without this option, `archive_attribution_check` is null, not implicitly verified.

The generator now also enforces the bounded candidate selection policy and
notice coverage. A chosen license must match the policy for the unchanged
publisher declaration. Unknown declarations or changed selections stop packaging
until reviewed. This policy covers the current graph and is **not** a general SPDX
parser. Existing choices are unchanged; legacy `MIT/Apache-2.0` is interpreted
explicitly as an alternative. No slash normalization is applied to arbitrary
future declarations.

`AND` preserves simultaneous obligations; `OR` provides alternatives; exceptions
belong to their license branch. The host policy preserves both terms for ring
and matchit and selects MIT plus Unicode for ICU's compound expression. It
selects the independent MIT alternative for the two declarations that also
offer Apache with an LLVM exception. See the
[SPDX expression specification](https://spdx.github.io/spdx-spec/v2.3/SPDX-license-expressions/).

Each component must still link its own retained candidate texts, even when
another component references identical bytes. Ring's source headers and fiat
AUTHORS have a dedicated native section; Rust library terms have their own
toolchain section. Missing, duplicate or misplaced sections and unmatched text
links stop packaging. This verifies retention and navigation, not that a file's
name or presence establishes all applicable rights.

## Candidate choices

| Selected expression | Registry packages |
| --- | ---: |
| MIT | 174 |
| Unicode-3.0 | 18 |
| Apache-2.0 | 5 |
| ISC | 2 |
| Zlib | 1 |
| MIT AND BSD-3-Clause | 1 |
| Apache-2.0 AND ISC | 1 |
| BSD-3-Clause | 1 |
| MIT AND Unicode-3.0 | 1 |
| CDLA-Permissive-2.0 | 1 |

The [ledger](../third-party/core-license-choices.json) and
[per-component index](core-third-party-notices.md) identify exact packages/texts.
All supplied alternatives remain retained; the table does not replace native
per-file terms or the Rust standard-library notice.

## Distribution work implied by the retained terms

These are operational review actions based on the retained original terms.
Use the exact component texts to resolve applicability, not only the generic
license identifier or summaries below.

| Terms | Required review/action for this binary plan | Current retained material |
| --- | --- | --- |
| MIT | Preserve the copyright and permission notice with substantial distributed portions. | Original component texts, including combined-license files such as chrono. |
| Apache-2.0 | Supply terms; preserve applicable upstream attributions/NOTICE; mark modified files when distributing them; review patent and trademark conditions. | Urdia LICENSE/NOTICE, selected upstream texts and native source headers. Relevant upstream NOTICE and modification review remain open. |
| ISC | Preserve copyright and permission notices. | ISC component texts and ring's LICENSE-other-bits/source headers. |
| BSD-3-Clause | Reproduce copyright, conditions and disclaimer in binary distribution materials; respect the non-endorsement clause. | matchit's LICENSE.httprouter and the other selected BSD text. |
| Zlib | Do not misrepresent origin; mark altered source; preserve the notice in source distributions. | Original selected component text; source distribution is a separate scope. |
| Unicode-3.0 | Carry the copyright/permission notice with data/software or documentation; respect restrictions on promotional use of names. | Original ICU/Unicode texts, including IBM attribution where supplied. |
| CDLA-Permissive-2.0 | Make the agreement available with shared data. | webpki-roots' original agreement shipped with the core notices. |

Primary reference terms: [Apache](https://www.apache.org/licenses/LICENSE-2.0),
[MIT](https://opensource.org/license/mit),
[Unicode](https://www.unicode.org/license.txt) and
[CDLA](https://cdla.dev/permissive-2-0/). The retained version-specific bytes remain
the package evidence; a newer generic text does not replace them.

## Bounded exceptions and remaining decisions

- ACP SDK/derive/schema use recovered root licenses at their recorded upstream
  revisions. Their publisher declarations and retained license copies agree;
  A pinned-source review now matches 122 original package files (87 SDK, three
  derive, 32 schema), original manifests and both retained root licenses at the
  recorded revisions. SDK/derive inherit the workspace Apache-2.0 declaration;
  schema declares it directly. Normalized manifests, Cargo.lock and VCS metadata
  are excluded from the original-source match. See the
  source applicability record (private review material omitted from this snapshot).
- Ring's top-level declaration is not exhaustive native attribution. BoringSSL,
  once_cell and fiat have their own retained texts/headers. The ten shipped fiat
  files match individual upstream blobs, supporting recovered AUTHORS; the
  crate's VCS dirty flag prevents treating the entire crate as that clean Git
  revision. Leading headers do not establish exhaustive inline-source coverage.
- Rust COPYRIGHT-library.html and twelve supplied terms are deliberately
  overinclusive. Determine actual component applicability before making claims
  about every listed license; the retained HTML includes components outside
  Cargo's registry graph. These bytes are hashed installed documentation, not
  a verified original rustup distribution archive.
- Host libc, loader, compiler runtime and SQLite are dynamically required but
  not shipped. If bundled later, add their distribution/source obligations and
  provenance. SQLCipher is not selected in the current build.
- Optional wheels, external agents and model weights are outside the core
  package. Their inventory gaps and provenance checks do not authorize bundling
  them. A source archive also needs its own inclusion/modification/privacy review.

The technical gate can pass while `text_applicability_approved`,
`distribution_approval` and `publication_allowed` remain false. Final review
must record the exact candidate, relevant upstream NOTICE/modification findings,
native/library applicability and a distribution decision. Production trust,
platform support and owner release authorization are independent gates. See the
release checklist (private review material omitted from this snapshot).

New private core builds record [linker section observations](core-link-observations.md) bound to
the executable and package evidence; source attribution and redistribution review
remain separate release gates.
