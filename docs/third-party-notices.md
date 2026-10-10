# Third-party text collection and release procedure

The [notice inventory](../THIRD_PARTY_NOTICES.md) and
[JSON manifest](../third-party/inventory.json) preserve candidate upstream texts.
They do not certify license compatibility or completeness of a distribution.

## Recorded inputs and acquisition

The manifest records the baseline commit and SHA-256 of Cargo.lock, both Python
requirements files, both selected platform hash profiles, and the pip bootstrap.
Rust inputs come from `cargo metadata --locked --all-features --format-version 1`:
274 registry packages, including target/feature branches absent from the resolved
234-package cargo-deny audit. Ten workspace packages are Urdia, not third-party
registry entries. This inventory is not advisory coverage or binary inclusion proof.

All 274 cached `.crate` archives were hashed against Cargo.lock checksums before
reading their regular members. The 47 unique selected wheel downloads were
checked against the exact URLs, sizes, and SHA-256 values in the dated
Laya (private review material omitted from this snapshot) and
TypeLLM (private review material omitted from this snapshot) artifact evidence. Collection
installed or imported no wheel code. Normalized name/version duplicates required
the same artifact hash before sharing a record.

Candidate selection matches LICENSE/LICENCE/NOTICE/COPYING/COPYRIGHT filenames
(case insensitive, optionally followed by a dot, underscore, or hyphen suffix),
and files in `licenses`/`licences` directories. Code/binary extensions are excluded.
The initial filename heuristic was extended manually for r-efi AUTHORS; exact
workspace/source provenance resolves rsqlite-vfs and tokenizers. See the
[package plan](release-package.md) for current resolution.
Archive paths are checked for traversal; individual text reads are bounded to
4 MiB. This heuristic can miss unconventional names and inline attribution.
Python wheel `License-File` metadata was checked at both standard and legacy
locations; no declared file was missing. Tokenizers declares no such file in the wheel; its exact-version sdist license
and metadata are now retained separately. The [PyPA metadata specification](https://packaging.python.org/en/latest/specifications/core-metadata/#license-file-multiple-use)
defines the field; its presence alone does not establish completeness.

For four Rust packages missing archive candidates, collection recovered five
root-file references from the exact `.cargo_vcs_info.json` revisions. GitHub
root-tree blob SHA-1 and retained SHA-256 were checked. The manifest distinguishes
these supplements from archive contents; the repository root license still needs
package-scope review. r-efi's inspected root contained no matching file;
rsqlite-vfs had no declared repository URL in the initial collection. The follow-up
verified four archive files against its embedded revision in the upstream workspace
before retaining the inherited root license and manifests.
Two model READMEs were fetched from already pinned Hugging Face revisions with a
1 MiB bound. Only cards were acquired, not weights or additional runtime files.

## Retention and verification

`third-party/texts/<SHA256>.txt` contains original bytes. Deduplication does not
remove attribution: the manifest retains every original path and package mapping.
Scoped `.gitattributes` disables line-ending conversion and whitespace diagnostics
for these upstream copies only. Do not reformat or normalize their contents.
The collector was a disposable acquisition script, not an installed project tool;
this snapshot is not yet an automated license-completeness gate.

To verify the retained snapshot without network access:

```sh
python3 - <<'PYVERIFY'
import hashlib, json
from pathlib import Path
root = Path.cwd()
manifest = json.loads((root / 'third-party/inventory.json').read_text())
refs = []
for group in ('rust', 'python'):
    for package in manifest[group]:
        refs.extend(package['files'])
        refs.extend(package.get('embedded_headers', []))
        refs.extend(package.get('supplemental_upstream', {}).get('files', []))
for toolchain in manifest.get('toolchains', []):
    refs.extend(toolchain['files'])
refs.extend(card for card in manifest['model_cards'] if 'retained_path' in card)
for ref in refs:
    path = Path(ref['retained_path'])
    assert path.parent == Path('third-party/texts')
    data = (root / path).read_bytes()
    assert len(data) == ref['size_bytes']
    assert hashlib.sha256(data).hexdigest() == ref['sha256']
    assert path.stem == ref['sha256']
expected = {ref['retained_path'] for ref in refs}
actual = {str(path.relative_to(root)) for path in (root / 'third-party/texts').glob('*.txt')}
assert expected == actual
for path, digest in manifest['input_sha256'].items():
    assert hashlib.sha256((root / path).read_bytes()).hexdigest() == digest
assert len(actual) == manifest['counts']['unique_retained_texts']
print('Retained texts, mappings, and input hashes verified')
PYVERIFY
```

## Next distribution gate

1. Choose the actual source archive, binaries, supported targets, and optional
   bundles. Record which dependencies, assets, and system libraries each includes.
2. Review the now-resolved exact-version candidates and supplements; inspect
   embedded headers, bundled native code, and model terms where included.
3. Record applicable license choices and obligations per included component;
   preserve all required licenses and attribution/NOTICE content. Use the actual
   upstream terms, including the [Apache license](https://www.apache.org/licenses/LICENSE-2.0).
   No compatibility conclusion follows from this collection.
4. Produce notices for each selected distribution and check the packaged files
   against the inclusion manifest. Recollect when any input hash changes.
5. Close the release checklist separately; publication requires owner authorization.
