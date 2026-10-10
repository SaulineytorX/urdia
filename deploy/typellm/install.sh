#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo "run this installer as root" >&2
    exit 1
fi

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
install_root=/opt/urdia/typellm
venv="$install_root/venv"
secrets_dir=/etc/urdia/secrets
credential="$secrets_dir/typellm.key"

install -d -m 0755 -o root -g root "$install_root"
python3 -I -m venv "$venv"
"$project_dir/scripts/install-python.sh" "$venv" typellm
"$venv/bin/python" -I -c 'import typellm; from importlib.metadata import version; assert version("typellm") == "0.2.6"'

install -m 0755 -o root -g root "$project_dir/deploy/typellm/start.sh" "$install_root/start.sh"
install -m 0644 -o root -g root "$project_dir/deploy/typellm/serve.py" "$install_root/serve.py"
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
    "$project_dir/deploy/typellm/urdia-typellm.service" \
    /etc/systemd/system/urdia-typellm.service
install -m 0644 -o root -g root \
    "$project_dir/deploy/typellm/typellm.conf.example" \
    /etc/urdia/typellm.conf.example
systemctl daemon-reload
printf '%s\n' 'Installed but not enabled. Pin a served model in /etc/urdia/typellm.conf, then enable explicitly.'
