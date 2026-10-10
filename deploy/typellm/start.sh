#!/bin/sh
set -eu

credential="${CREDENTIALS_DIRECTORY:?missing systemd credential directory}/typellm_key"
if [ ! -r "$credential" ]; then
    echo "TypeLLM bridge credential is unavailable" >&2
    exit 1
fi
TYPELLM_BRIDGE_API_KEY=$(cat "$credential")
if [ -z "$TYPELLM_BRIDGE_API_KEY" ]; then
    echo "TypeLLM bridge credential is empty" >&2
    exit 1
fi
export TYPELLM_BRIDGE_API_KEY
unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy
export NO_PROXY=127.0.0.1,::1
export no_proxy="$NO_PROXY"
exec /opt/urdia/typellm/venv/bin/python -I /opt/urdia/typellm/serve.py
