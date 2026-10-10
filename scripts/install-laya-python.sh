#!/bin/sh
# Compatibility entry point for the shared Laya package profile.
set -eu
if [ "$#" -ne 1 ]; then
    echo "usage: install-laya-python.sh VENV" >&2
    exit 2
fi
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$script_dir/install-python.sh" "$1" laya
