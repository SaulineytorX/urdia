#!/usr/bin/env python3
"""Start an explicit project's local browser conversation on Linux or WSL."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def executable(value, fallback):
    if value:
        selected = value
    elif fallback.exists() or fallback.is_symlink():
        selected = fallback
    else:
        selected = shutil.which(fallback.name) or fallback
    path = Path(selected).expanduser().resolve(strict=True)
    if not path.is_file() or not os.access(path, os.X_OK):
        raise ValueError('required executable unavailable; build release binaries first')
    return path


def workspace_path(value):
    if re.match(r'^[A-Za-z]:[\\/]', value):
        raise ValueError('use the WSL path, such as /mnt/c/Users/you/project')
    path = Path(value).expanduser().resolve(strict=True)
    if not path.is_dir():
        raise ValueError('workspace must be an existing directory')
    return path


def state_directory(workspace, provider):
    base = Path(os.environ.get('XDG_STATE_HOME', str(Path.home() / '.local/state'))).expanduser()
    if not base.is_absolute():
        raise ValueError('XDG_STATE_HOME must be absolute')
    key = hashlib.sha256((str(workspace) + '\0' + provider).encode()).hexdigest()[:24]
    directory = base / 'urdia/projects' / key
    if directory.resolve().is_relative_to(workspace):
        raise ValueError('session data must stay outside the project')
    # Refuse symlink parents instead of following a redirected state location.
    for part in [directory, *directory.parents]:
        if part.is_symlink():
            raise ValueError('symlink session data path refused')
    return directory


def protect_directory(path):
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    info = path.stat()
    if (not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid()
            or info.st_mode & 0o077):
        raise ValueError('session directory must be owned by you with mode 0700')
    for name in ('events.sqlite', 'events.sqlite-wal', 'events.sqlite-shm', 'demo-provider.toml'):
        item = path / name
        if item.is_symlink():
            raise ValueError('symlink session file refused')
        if item.exists() and (not item.is_file() or item.stat().st_uid != os.getuid()):
            raise ValueError('invalid session file')


def demo_config(fake):
    return ('[[provider]]\nid = "preview-demo"\nname = "Offline demo (not AI)"\ncommand = ' +
            json.dumps(str(fake), ensure_ascii=True) + '\nargs = ["--stream"]\n').encode()


def available(binary, config, provider, profile_store=None, agent_store=None):
    command = [str(binary)]
    if agent_store: command += ['--agent-store', str(agent_store)]
    if config:
        command += ['--config', str(config)]
    if profile_store:
        command += ['--profile-store', str(profile_store)]
    result = subprocess.run([*command, 'providers'], stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, timeout=10, check=False)
    if result.returncode or len(result.stdout) > 1024 * 1024:
        raise ValueError('provider inspection failed; run urdia doctor with your config')
    lines = result.stdout.decode('utf-8').splitlines()
    if not lines or lines[0] != 'ID\tSTATUS\tAUTH\tINFERENCE\tTRANSPORT\tNAME':
        raise ValueError('unsupported provider listing')
    if not any(len(fields := line.split('\t')) == 6 and fields[0] == provider
               and fields[1] == 'available' for line in lines[1:]):
        raise ValueError('selected provider unavailable; install it inside WSL or supply --config')


def selected_profile(binary, config, profile=None, profile_store=None, agent_store=None):
    """Read the host's validated catalog; never parse or echo source configuration."""
    if not config and not profile_store and not agent_store:
        if profile:
            raise ValueError('--profile requires --config, --profile-store or --agent-store')
        return None
    command = [str(binary)]
    if agent_store: command += ['--agent-store', str(agent_store)]
    if config: command += ['--config', str(config)]
    if profile_store: command += ['--profile-store', str(profile_store)]
    result = subprocess.run([*command, 'profiles', '--json'],
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                            timeout=10, check=False)
    if result.returncode or len(result.stdout) > 1024 * 1024:
        raise ValueError('profile inspection failed; check your Urdia config')
    try:
        entries = json.loads(result.stdout)
        if not isinstance(entries, list) or len(entries) > 64:
            raise ValueError()
        matches = [entry for entry in entries if
                   (entry['profile']['id'] == profile if profile else entry['default'] is True)]
        if not matches and not profile:
            return None
        if len(matches) != 1:
            raise ValueError()
        entry = matches[0]
        if entry['executable_available'] is not True:
            raise ValueError('selected profile executable unavailable')
        for key in ('id', 'provider'):
            value = entry['profile'][key]
            if not isinstance(value, str) or not re.fullmatch(r'[a-z][a-z0-9]*(?:-[a-z0-9]+)*', value):
                raise ValueError()
        return entry['profile']['id'], entry['profile']['provider']
    except (KeyError, TypeError, json.JSONDecodeError, ValueError):
        raise ValueError('selected profile unavailable or catalog invalid; check profiles --json') from None


def command(binary, workspace, directory, provider, config=None, profile=None):
    args = [str(binary)]
    if config:
        args += ['--config', str(config)]
    selection = ['--profile', profile] if profile else ['--provider', provider]
    return [*args, 'chat', *selection, '--workspace', str(workspace),
            '--database', str(directory / 'events.sqlite'), '--permissions-ui',
            '--bind', '127.0.0.1:0']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workspace', required=True, help='existing Linux/WSL project path')
    parser.add_argument('--urdia', help='explicit trusted Urdia executable')
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument('--provider', help='explicit ACP provider ID; overrides configured default')
    selection.add_argument('--profile', help='named execution profile from --config or --profile-store')
    parser.add_argument('--config', type=Path, help='external ACP provider configuration')
    parser.add_argument('--agent-store', type=Path, help='private launch settings file; registered agents apply after restart')
    parser.add_argument('--profile-store', type=Path, help='private profile-only settings file; dashboard saves apply after restart')
    parser.add_argument('--demo', action='store_true', help='offline streaming echo; no AI or file edits')
    parser.add_argument('--fake-agent', help='reference agent executable, only with --demo')
    parser.add_argument('--check', action='store_true', help='check readiness without starting a session or writing data')
    parser.add_argument('--verification-ui', action='store_true', help='offer explicitly confirmed Python project and calculator checks')
    parser.add_argument('--advisory-ui', action='store_true', help='enable explicit browser advice from host-configured engines')
    parser.add_argument('--advisory-profiles', action='store_true', help='recommend configured profiles; requires --advisory-ui')
    args = parser.parse_args()
    if args.advisory_profiles and not args.advisory_ui:
        raise ValueError('--advisory-profiles requires --advisory-ui')
    if sys.platform != 'linux':
        raise ValueError('run this launcher inside Ubuntu/WSL, not Windows Python')
    if (args.demo and (args.provider or args.profile or args.config or args.profile_store or args.agent_store)) or (args.fake_agent and not args.demo):
        raise ValueError('--demo cannot mix real-provider options; --fake-agent requires --demo')
    workspace = workspace_path(args.workspace)
    binary = executable(args.urdia, ROOT / 'target/release/urdia')
    provider = 'preview-demo' if args.demo else args.provider or 'qwen'
    if not re.fullmatch(r'[a-z][a-z0-9]*(?:-[a-z0-9]+)*', provider):
        raise ValueError('invalid provider ID')
    config = args.config.expanduser().resolve(strict=True) if args.config else None
    profile_store = args.profile_store.expanduser().absolute() if args.profile_store else None
    agent_store = args.agent_store.expanduser().absolute() if args.agent_store else None
    profile = selected_profile(binary, config, args.profile, profile_store, agent_store) if not args.demo and not args.provider else None
    if profile:
        profile_id, provider = profile
    else:
        profile_id = None
    directory = state_directory(workspace, 'profile:' + profile_id if profile_id else provider)
    fake = executable(args.fake_agent, ROOT / 'target/release/urdia-fake-agent') if args.demo else None
    if not args.demo:
        available(binary, config, provider, profile_store, agent_store)
    if args.check:
        print('Ready for local preview; provider authentication and WSL browser access still need a live test.')
        selection_label = 'Profile: ' + profile_id if profile_id else 'Explicit provider: ' + provider
        print('Offline demo (not AI).' if args.demo else selection_label)
        return
    protect_directory(directory)
    if fake:
        config = directory / 'demo-provider.toml'
        data = demo_config(fake)
        if config.exists():
            if config.read_bytes() != data:
                raise ValueError('demo executable changed; review and remove the old demo-provider.toml first')
        else:
            fd = os.open(config, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, 'wb') as output:
                output.write(data)
    print('Project: ' + str(workspace))
    print('Mode: offline streaming demo (not AI, no project edits).' if args.demo else 'Agent: ' + provider)
    print('Open the private URL printed below in your Windows browser. Keep this terminal open; Ctrl+C stops it.')
    print('Manual sending is default. Optional experimental Jev selection runs only a submitted task. Tool permissions stay separate; prompt content is not retained.')
    sys.stdout.flush()
    argv = command(binary, workspace, directory, provider, config, profile_id)
    if agent_store:
        argv += ['--agent-store', str(agent_store)]
    if profile_store:
        argv += ['--profile-store', str(profile_store)]
    if profile_id and not args.profile:
        # Preserve default selection identity in the host instead of pinning an explicit flag.
        index = argv.index('--profile')
        del argv[index:index + 2]
    if args.verification_ui:
        argv.append("--verification-ui")
    if args.advisory_ui:
        argv.append('--advisory-ui')
    if args.advisory_profiles:
        argv.append('--advisory-profiles')
    os.execv(str(binary), argv)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        # Do not echo provider stderr or configuration values.
        print('start-project: ' + (str(error) if isinstance(error, ValueError) else
                                  'local prerequisite unavailable; check paths and permissions'), file=sys.stderr)
        raise SystemExit(1)
