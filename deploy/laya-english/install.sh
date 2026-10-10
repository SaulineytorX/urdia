#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo "run this installer as root" >&2
    exit 1
fi

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
install_root=/opt/urdia/laya-english
venv="$install_root/venv"
model_dir="$install_root/model"
secrets_dir=/etc/urdia/secrets
credential="$secrets_dir/laya-english.key"
model_revision=55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851

install -d -m 0755 -o root -g root "$install_root"
python3 -I -m venv "$venv"
"$project_dir/scripts/install-laya-python.sh" "$venv"
"$venv/bin/python" -I -c 'import laya; assert laya.__version__ == "0.3.20"'

install -d -m 0755 -o root -g root "$model_dir"
MODEL_DIR="$model_dir" MODEL_REVISION="$model_revision" \
    "$venv/bin/python" - <<'PY'
import os
from huggingface_hub import snapshot_download

snapshot_download(
    repo_id="convaiinnovations/laya",
    revision=os.environ["MODEL_REVISION"],
    local_dir=os.environ["MODEL_DIR"],
    allow_patterns=[
        "rl_agent_config.json",
        "model.safetensors",
        "tokenizer/tokenizer.json",
        "tokenizer/tokenizer_config.json",
        "encoder/config.json",
    ],
)
PY

"$venv/bin/python" -I "$project_dir/scripts/verify-model.py" \
    --manifest "$project_dir/deploy/laya-english/model-integrity.json" \
    --model-dir "$model_dir" \
    --repository convaiinnovations/laya \
    --revision "$model_revision"

install -m 0755 -o root -g root "$project_dir/deploy/laya-english/start.sh" "$install_root/start.sh"
install -m 0644 -o root -g root "$project_dir/deploy/laya-english/serve.py" "$install_root/serve.py"
find "$install_root" -type d -exec chmod go-w {} +
find "$install_root" -type f -exec chmod go-w {} +

install -d -m 0700 -o root -g root "$secrets_dir"
if [ ! -e "$credential" ]; then
    umask 077
    generated_key=$(openssl rand -hex 32)
    printf '%s' "$generated_key" >"$credential"
fi
chown root:root "$credential"
chmod 0600 "$credential"

install -m 0644 -o root -g root \
    "$project_dir/deploy/laya-english/urdia-laya-english.service" \
    /etc/systemd/system/urdia-laya-english.service
systemctl daemon-reload
systemctl enable urdia-laya-english.service
systemctl restart urdia-laya-english.service
