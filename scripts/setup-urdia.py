#!/usr/bin/env python3
"""Guided, user-local source setup for Linux/WSL; no elevated or account operations."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import stat
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('project_launcher', ROOT / 'scripts/start-project.py')
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


def prerequisites():
    missing = []
    if sys.platform != 'linux' or platform.machine() != 'x86_64':
        missing.append('Use Ubuntu 24.04 x86_64, inside WSL on Windows.')
    for program in ('cargo', 'rustup', 'cc', 'pkg-config'):
        if not shutil.which(program):
            missing.append('Missing ' + program + '.')
    if shutil.which('pkg-config'):
        result = subprocess.run(['pkg-config', '--exists', 'sqlite3'], stdout=subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL, timeout=10)
        if result.returncode:
            missing.append('Missing SQLite development files (libsqlite3-dev).')
    if shutil.which('rustup'):
        match = re.search(r'^channel\s*=\s*"([0-9.]+)"', (ROOT / 'rust-toolchain.toml').read_text(), re.M)
        if not match:
            raise ValueError('source toolchain definition is invalid')
        version = match.group(1)
        result = subprocess.run(['rustup', 'toolchain', 'list'], stdout=subprocess.PIPE,
                                stderr=subprocess.DEVNULL, timeout=10)
        if result.returncode or not any(line.split()[0].startswith(version + '-') or line.split()[0] == version
                                        for line in result.stdout.decode().splitlines() if line.split()):
            missing.append('Install the pinned Rust toolchain: rustup toolchain install ' + version)
    return missing


def confirm(message):
    try:
        return input(message + ' [y/N] ').strip().lower() in ('y', 'yes')
    except EOFError:
        return False


def build():
    if not confirm('Build this source now? Downloads locked public dependencies and may take a few minutes.'):
        return False
    env = {key: value for key, value in os.environ.items()
           if key in ('PATH', 'HOME', 'CARGO_HOME', 'RUSTUP_HOME', 'TMPDIR', 'LANG', 'LC_ALL')}
    print('Preparing Urdia. Please keep this terminal open.', flush=True)
    with tempfile.TemporaryFile() as log:
        result = subprocess.run(['cargo', 'build', '--release', '--locked', '--jobs', '2', '--target-dir', str(ROOT / 'target')],
                                cwd=ROOT, env=env, stdout=log, stderr=log)
    if result.returncode:
        raise ValueError('Build failed. Check the source installation guide; no agent was started.')
    return True


def choose_agent(demo=False):
    if demo:
        return 'demo'
    choices = [('demo', 'Offline demo — not AI; no project edits')]
    if shutil.which('qwen'):
        choices.append(('qwen', 'Qwen Code — installed; sign-in is not verified'))
    if shutil.which('codex'):
        # The bridge pins this version; do not silently launch an incompatible CLI.
        result = subprocess.run([shutil.which('codex'), '--version'], stdout=subprocess.PIPE,
                                stderr=subprocess.DEVNULL, timeout=10)
        if result.returncode == 0 and result.stdout.strip() == b'codex-cli 0.159.2':
            choices.append(('codex', 'Codex — installed compatible bridge; sign-in is not verified'))
        else:
            print('Codex was found but its version is not supported by this bridge. See docs/codex.md.')
    print('\nChoose how to start:')
    for index, (_, label) in enumerate(choices, 1):
        print(f'  {index}. {label}')
    try:
        answer = input('Choice [1]: ').strip() or '1'
    except EOFError:
        raise ValueError('Setup cancelled; rerun in an interactive terminal.') from None
    if not answer.isdecimal() or not 1 <= int(answer) <= len(choices):
        raise ValueError('Choose one of the listed options; no agent was started.')
    return choices[int(answer) - 1][0]


def project(value):
    if value:
        return launcher.workspace_path(value)
    default = Path.home() / 'projects/urdia-preview-project'
    try:
        value = input(f'Project folder [{default}]: ').strip() or str(default)
    except EOFError:
        raise ValueError('Setup cancelled; rerun in an interactive terminal.') from None
    if re.match(r'^[A-Za-z]:[\\/]', value):
        raise ValueError('Use a Linux/WSL path, such as /mnt/c/Users/you/project.')
    path = Path(value).expanduser().absolute()
    if not path.exists():
        if not confirm('Create this empty project folder?'):
            raise ValueError('No project created; choose an existing folder next time.')
        path.mkdir(parents=True, exist_ok=False)
    return launcher.workspace_path(str(path))


def private_config(path, content):
    # Existing settings must match; never overwrite a redirected or edited file.
    if path.exists() or path.is_symlink():
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(fd, 'rb') as file:
            info = os.fstat(file.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1 or info.st_mode & 0o077:
                raise ValueError('Private setup configuration has unsafe ownership or permissions.')
            if file.read(16385) != content or len(content) > 16384:
                raise ValueError('Saved launch configuration differs. Review it before rerunning setup.')
        return
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'wb') as file:
        file.write(content)


def launch_arguments(binary, workspace, agent, fake=None):
    args = [sys.executable, '-I', '-B', str(ROOT / 'scripts/start-project.py'),
            '--workspace', str(workspace), '--urdia', str(binary)]
    if agent == 'demo':
        return args + ['--demo', '--fake-agent', str(fake), '--verification-ui']
    settings = launcher.state_directory(workspace, 'guided-settings')
    launcher.protect_directory(settings)
    args += ['--provider', agent, '--verification-ui',
             '--agent-store', str(settings / 'agents.toml'),
             '--profile-store', str(settings / 'profiles.toml')]
    if agent == 'codex':
        # Reuse an operator-registered Codex rather than adding a duplicate ID.
        saved_agents = settings / 'agents.toml'
        if saved_agents.exists():
            try:
                launcher.available(binary, None, 'codex', agent_store=saved_agents)
            except ValueError:
                pass  # The host still validates/refuses any invalid saved catalog.
            else:
                print('Using your saved Codex registration. Review it in Setup guide.')
                return args
        program = shutil.which('codex')
        if not program:
            raise ValueError('Codex is no longer available.')
        content = ('[[provider]]\nid = "codex"\nname = "Codex native (experimental)"\ncommand = '
                   + json.dumps(sys.executable) + '\nargs = '
                   + json.dumps(['-I', '-B', str(ROOT / 'scripts/codex-acp.py'), '--codex', program]) + '\n').encode()
        config = settings / 'codex-provider.toml'
        private_config(config, content)
        args += ['--config', str(config)]
    return args


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='read-only prerequisite check; no downloads or sessions')
    parser.add_argument('--workspace', help='existing project; otherwise choose interactively')
    parser.add_argument('--demo', action='store_true', help='select offline demo without agent discovery')
    parser.add_argument('--no-build', action='store_true', help='explicitly reuse an existing build; source freshness not verified')
    parser.add_argument('--urdia', help='explicit trusted executable, only with --no-build')
    parser.add_argument('--fake-agent', help='explicit reference executable, only with --no-build --demo')
    args = parser.parse_args()
    if sys.platform != 'linux':
        raise ValueError('Open Ubuntu/WSL first; this is not a native Windows installer.')
    if (args.urdia and not args.no_build) or (args.fake_agent and not (args.no_build and args.demo)):
        raise ValueError('Executable overrides require --no-build; --fake-agent also requires --demo.')
    print('Urdia · Guided setup\nLocal workspace. Your project. Your control.\n')
    if not args.no_build or args.check:
        missing = prerequisites()
        if missing:
            for item in missing:
                print('  • ' + item)
            print('\nSee docs/installation.md for prerequisite installation. Setup does not run sudo or install agents.')
            return 1
        print('Prerequisites ready. No accounts or credentials inspected.')
    if args.check:
        return 0
    selected_workspace = launcher.workspace_path(args.workspace) if args.workspace else None
    if not args.no_build and not build():
        print('Cancelled. No session started.')
        return 0
    if args.no_build:
        print('Reusing your selected build; this does not verify it matches the source.')
    binary = launcher.executable(args.urdia, ROOT / 'target/release/urdia')
    agent = choose_agent(args.demo)
    workspace = selected_workspace or project(None)
    fake = launcher.executable(args.fake_agent, ROOT / 'target/release/urdia-fake-agent') if agent == 'demo' else None
    print('\nProject: ' + str(workspace))
    print('Mode: offline demo (not AI; no project edits).' if agent == 'demo' else
          'Mode: ' + agent + '. This uses its own account; entitlement and quota are not verified.')
    print('The browser URL stays local and private. Manual sending is default; experimental Jev selection needs configured profiles.')
    print('Configure Jev/Laya in Decision engines; no API key is requested here.')
    if not confirm('Start this local workspace now?'):
        print('Cancelled. No session or launch settings created.')
        return 0
    command = launch_arguments(binary, workspace, agent, fake)
    # Argument arrays only; no shell, privileged install, account login or auto-open URL helper.
    sys.stdout.flush()
    os.execv(sys.executable, command)


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print('\nSetup stopped.', file=sys.stderr)
        raise SystemExit(130)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print('Setup: ' + (str(error) if isinstance(error, ValueError) else
                          'A local prerequisite failed; check the installation guide.'), file=sys.stderr)
        raise SystemExit(1)
