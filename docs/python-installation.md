# Pinned Python inputs for decision services

The optional Laya and TypeLLM deployments target CPython 3.12 on Linux x86_64 with glibc
2.28 or newer. The checked environment is Ubuntu 24.04. These requirements do
not apply to Urdia's Rust CLI or to external ACP providers. Other Python
versions, architectures, musl systems, and operating systems require separately
selected and tested wheel sets; the helper rejects them before installing.

## Inputs

- `deploy/laya/requirements.lock` records the 43 intended package versions.
- `deploy/laya/requirements-linux-x86_64-py312.lock` selects one exact wheel URL
  and SHA-256 per package for this target.
- `deploy/typellm/requirements.lock` records the 29 intended dependencies.
- `deploy/typellm/requirements-linux-x86_64-py312.lock` selects those 29 wheels
  plus TypeLLM 0.2.6 with exact URLs/SHA-256.
- `deploy/python/bootstrap.lock` pins the shared pip wheel.

Both Laya variants use the same package inputs. TypeLLM has a separate profile
because some shared packages have different pinned versions. PyPI package artifacts use
`files.pythonhosted.org`; CPU PyTorch uses an explicit official
`download.pytorch.org` wheel URL. No index search or extra-index fallback selects
packages during installation. The downloaded bytes must match the checked-in
hashes. If an artifact is unavailable or mismatched, installation fails rather
than selecting another file or source distribution.

## Package-only installation

The shared helper changes only the virtualenv passed to it. It does not start a
service, create credentials, download models, or run inference. For a disposable
check, use a fresh temporary environment:

```sh
set -eu
python_env_root=$(mktemp -d)
trap 'rm -rf -- "$python_env_root"' 0
python3 -I -m venv "$python_env_root/venv"
sh scripts/install-python.sh "$python_env_root/venv" typellm
"$python_env_root/venv/bin/python" -I -m pip check
```

Use `laya` instead of `typellm` for the shared Laya profile. The existing
`install-laya-python.sh VENV` command remains a compatibility entry point.
Unknown profiles are rejected before accessing the virtualenv.

The helper first validates the interpreter/platform/venv, installs the pinned
pip wheel, installs the selected profile (43 Laya wheels or 30 TypeLLM wheels),
then runs `pip check`.
Commands use `--no-index --only-binary=:all: --require-hashes --force-reinstall`,
an isolated Python interpreter, a filtered environment, a temporary home/work
directory, and disabled pip configuration. Dependency resolution remains enabled: a
missing or incompatible declared dependency fails; it is not hidden with
`--no-deps`. Force-reinstallation checks the requested wheel bytes even when
the same versions are already present.

This is a package input check, not a full service deployment. The source service
installers invoke it before later model or systemd setup. Laya subsequently
downloads/verifies its pinned models and may restart its services; TypeLLM
installs a unit without enabling it and requires a separately operated SGLang
model and tokenizer for any real execution. Running those installers
against an existing installation changes that installation; do package-only
validation in a disposable environment first.

## Updating the inputs

Update version intent and the selected wheel set together. Obtain each exact
release's official metadata, choose wheels compatible with the target, download
the bytes, compare size/SHA-256 and wheel Name/Version, and retain sanitized
source/digest evidence. Select CPU PyTorch independently from its official
index. Do not let a mixed-index resolver choose source provenance or silently
substitute an sdist. Re-run fresh installation, `pip check`, imports, negative
hash tests, and required repository checks before updating the source installers.

Hash identity is not a signature, a malware scan, a license conclusion, or
validation of model inference. The OS Python/venv/ensurepip and system libraries
remain prerequisites. Existing environments may contain additional packages;
this helper reinstalls the selected inputs but does not remove extras or prove
an existing environment is clean. Fresh-environment evidence has a narrower,
explicit package set.

The TypeLLM profile validates client/bridge packages only. It does not install
SGLang, provide accelerator hardware, fetch tokenizers/models, or establish
live model compatibility. Final license texts/notices, release provenance, and
a separately reviewed upgrade of active Laya virtualenvs remain pending. See
[optional input review](optional-deployment-audit.md) for advisory and
model-integrity coverage.
