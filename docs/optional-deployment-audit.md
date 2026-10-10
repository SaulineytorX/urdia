# Optional deployment inputs and integrity

Laya and TypeLLM are optional decision services. Their deployment dependencies
and models are separate from the Rust graph in [the dependency audit](dependency-audit.md).
A passing Rust check does not audit Python packages, model files, or their
redistribution terms. Semantic services remain advisory.

## Version/advisory review

The approved Python advisory tool for this slice is pip-audit 2.10.1 (Python
3.10 or newer). Use a disposable tool environment and an explicit public PyPI
index, not an installed decision-service virtualenv. The official tool wheel
SHA-256 is `99ef3f600a317c1945f1e89e227ef26e1c2d618429b8bd3fa6f4f7c440c4611a`.
The auditor's dependency environment is not a hash-locked production image.

Use exact inputs without resolving or installing the service packages:

```sh
pip-audit --version
pip-audit --requirement deploy/typellm/requirements.lock \
  --no-deps --disable-pip --strict --vulnerability-service pypi \
  --format json --desc off --aliases on
```

TypeLLM 0.2.6 must also be audited; the 29-line version-intent requirements file
alone does not include that wheel. The selected artifact lock includes it as
the thirtieth package. Keep Laya and
TypeLLM scans separate because two shared packages have different versions.
Never use `--fix` against a live service. `--no-deps --disable-pip` skips
resolution/installation, so the review does not prove dependency closure or
installer compatibility. These public metadata requests do not run inference.

The full Laya requirements scan with `--strict` fails because
`torch==2.14.0+cpu` is not a PyPI release. An explicitly recorded 42-package PyPI
subset can be reviewed separately. Never silently omit the CPU build or label
a partial scan as a complete clean result. Consult its official PyTorch index
and Core Metadata independently; the ordinary upstream `torch==2.14.0` result
is supplemental and does not verify the custom wheel's bytes.

Record tool version, UTC query time, requirement-file digest, source/service,
package counts, skipped/failed inputs, advisory IDs/aliases/fix versions, and
result digests. PyPI supplies live advisory data, not an immutable database
revision. Sanitize evidence; retain no credentials or raw environment output.

The source Laya lock now pins setuptools 83.0.0 after a disposable compatibility
exercise: imports, dependency consistency, offline package building, and
in-process HTTP validation without model loading passed. This version addresses
[GHSA-5rjg-fvgr-3xxf](https://github.com/advisories/GHSA-5rjg-fvgr-3xxf) and
[GHSA-h35f-9h28-mq5c](https://github.com/advisories/GHSA-h35f-9h28-mq5c).
The direct second advisory has a stale patched-version field; the official
[83.0.0 changelog](https://setuptools.pypa.io/en/stable/history.html#v83-0-0)
and PyPA advisory database identify the fix. A fresh release-metadata query
reports no advisory records for the 42 PyPI pins; custom CPU torch coverage
remains separate.

Existing deployments are not upgraded by a source-lock change. The two installed
Laya services still use 78.1.0; replacing their environments requires separate
validated deployment work. Full optional-deployment release validation remains
blocked by complete license/notices review, real model/runtime evidence, and
release provenance. Laya and TypeLLM now have separate platform-specific hashed
wheel profiles and a shared pinned pip bootstrap;
see [package installation](python-installation.md) for its scope and prerequisites. The compatibility exercise did not preload models
or run inference. No advisory was ignored.

## Model-file verification

Both Laya installers download exactly five runtime files at an immutable
revision and verify all five against the checked-in manifest before proceeding
to service installation/restart. Manifests include the checkpoint, encoder
configuration, agent configuration, tokenizer, and tokenizer configuration.
The verifier uses only the Python standard library, accesses no network,
imports no model package, and starts no service:

```sh
python3 -I scripts/verify-model.py \
  --manifest deploy/laya-english/model-integrity.json \
  --model-dir /path/to/model \
  --repository convaiinnovations/laya \
  --revision 55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851
```

For typed decisions, use `deploy/laya/model-integrity.json`, repository
`convaiinnovations/laya-typed-decisions`, and revision
`1a793eb568e6718f15941d08f85432581df534e3`.

Linux directory-descriptor traversal rejects symlinks and nonregular files;
size/count/manifest bounds limit the read. A mismatch, missing file, invalid
identity, or invalid manifest exits nonzero with a redacted error. Success
prints only the verified file count. This is a point-in-time comparison of
listed files, not a malware scan, a signature, permission to trust a model, or
protection against later file changes.

## Provenance and licensing limits

Both fixed model cards declare Apache-2.0. The manifest hashes were checked
against the fixed Hub metadata: LFS SHA-256 for weights and Git blob identifiers
for smaller local files, then SHA-256 for every manifest entry. No model weights
were downloaded during the review. A model card's declaration is not a complete
license-text/notice review for every dependency or bundled artifact.

Python distribution licenses differ: certifi declares MPL-2.0; NumPy and the
CPU PyTorch wheel declare combined license expressions, including bundled
components. Preserve and review upstream license texts and notices before
bundling these inputs; Urdia's Apache-2.0 license does not relicense them.

The Laya and TypeLLM installers now use exact wheel URLs and hashes, a pinned pip
bootstrap, and explicit PyPI/CPU artifact provenance for CPython 3.12 Linux
x86_64. Version intent remains separately recorded in each `requirements.lock`;
TypeLLM's artifact profile additionally includes its client wheel. Its
separately operated model, final third-party notices, and release provenance
remain to review. Package-only validation does not require installing,
restarting, or activating live services.
