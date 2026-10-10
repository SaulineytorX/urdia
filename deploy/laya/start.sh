#!/bin/sh
set -eu

credential="${CREDENTIALS_DIRECTORY:?missing systemd credential directory}/laya_key"
if [ ! -r "$credential" ]; then
    echo "Laya credential is unavailable" >&2
    exit 1
fi

LAYA_API_KEY=$(cat "$credential")
if [ -z "$LAYA_API_KEY" ]; then
    echo "Laya credential is empty" >&2
    exit 1
fi
export LAYA_API_KEY

exec /opt/urdia/laya/venv/bin/python -I /opt/urdia/laya/serve.py
