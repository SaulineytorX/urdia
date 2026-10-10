#!/bin/sh
# Install reviewed decision-service wheels on CPython 3.12 Linux x86_64.
# This helper never installs a service, accesses credentials, or loads a model.
set -eu

if [ "$#" -ne 2 ]; then
    echo "usage: install-python.sh VENV {laya|typellm}" >&2
    exit 2
fi
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
case "$2" in
    laya|typellm) profile="$2" ;;
    *) echo "unknown Python installation profile" >&2; exit 2 ;;
esac
venv=$(CDPATH= cd -- "$1" && pwd)
if [ ! -x "$venv/bin/python" ]; then
    echo "a Python virtualenv is required" >&2
    exit 1
fi
work_dir=$(mktemp -d /tmp/urdia-python-install.XXXXXXXX)
cleanup() {
    status=$?
    trap - 0
    if ! rm -rf -- "$work_dir"; then
        echo "failed to clean the temporary Python installer directory" >&2
        exit 1
    fi
    exit "$status"
}
trap cleanup 0
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -m 0700 "$work_dir/home"

run_python() {
    env -i PATH=/usr/bin:/bin HOME="$work_dir/home" TMPDIR="$work_dir" \
        PIP_CONFIG_FILE=/dev/null PYTHONDONTWRITEBYTECODE=1 \
        HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 \
        "$venv/bin/python" -I "$@"
}

# Reject an unsupported interpreter or a non-venv before installing anything.
run_python - "$venv" <<'CHECK'
import os
import platform
import sys

libc, version = platform.libc_ver()
try:
    libc_version = tuple(map(int, version.split(".")[:2]))
except ValueError:
    libc_version = ()
valid = (
    sys.implementation.name == "cpython"
    and sys.version_info[:2] == (3, 12)
    and platform.system() == "Linux"
    and platform.machine() == "x86_64"
    and libc == "glibc"
    and libc_version >= (2, 28)
    and sys.prefix != sys.base_prefix
    and os.path.realpath(sys.prefix) == os.path.realpath(sys.argv[1])
)
if not valid:
    raise SystemExit("requires a CPython 3.12 Linux x86_64 glibc >= 2.28 virtualenv")
CHECK

# Direct wheel URLs and mandatory hashes: neither command consults an index.
run_python -m pip --disable-pip-version-check --no-cache-dir install \
    --no-index --only-binary=:all: --require-hashes --force-reinstall \
    --requirement "$project_dir/deploy/python/bootstrap.lock"
run_python -m pip --disable-pip-version-check --no-cache-dir install \
    --no-index --only-binary=:all: --require-hashes --force-reinstall \
    --requirement "$project_dir/deploy/$profile/requirements-linux-x86_64-py312.lock"
run_python -m pip --disable-pip-version-check check
