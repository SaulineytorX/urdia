#!/usr/bin/env python3
"""Opt-in Linux vault acceptance with synthetic data, isolated HOME and private D-Bus."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[1]


def worker(args):
    root = args.worker.resolve()
    if (not root.name.startswith('urdia-native-vault-') or
            Path(os.environ.get('HOME','/missing')).resolve()!=root/'vault-home' or
            os.environ.get('DBUS_SESSION_BUS_ADDRESS','').split(',')[0]!='unix:path='+str(root/'runtime/bus')):
        raise ValueError('private vault isolation required')
    password = secrets.token_urlsafe(24)
    process = None
    results = []
    def stop():
        nonlocal process
        if process is not None and process.poll() is None:
            process.terminate()
            try: process.wait(timeout=3)
            except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=3)
        process = None
        time.sleep(.2)
    def start(value):
        nonlocal process
        (root/'control').mkdir(mode=0o700, exist_ok=True)
        with (root/'daemon.log').open('wb') as log:
            process = subprocess.Popen([str(args.daemon), '--foreground', '--components=secrets', '--unlock', '--control-directory', str(root/'control')], stdin=subprocess.PIPE, stdout=log, stderr=log)
        process.stdin.write(value.encode()); process.stdin.close()
        deadline = time.monotonic()+10
        while time.monotonic()<deadline:
            response = subprocess.run(['gdbus','call','--session','--dest','org.freedesktop.DBus','--object-path','/org/freedesktop/DBus','--method','org.freedesktop.DBus.NameHasOwner','org.freedesktop.secrets'],capture_output=True,timeout=3)
            if b'true' in response.stdout: return
            if process.poll() is not None: raise ValueError('native daemon failed')
            time.sleep(.1)
        raise ValueError('native daemon readiness failed')
    def phase(name, test_phase):
        started = time.monotonic()
        env = dict(os.environ, URDIA_NATIVE_VAULT_PHASE=test_phase)
        with (root/'test.log').open('wb') as log:
            result = subprocess.run([str(args.cargo),'test','-p','urdia','--bin','urdia','native_secret_service_acceptance','--locked','--','--ignored','--exact','credential_store::tests::native_secret_service_acceptance'],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=90)
        passed = result.returncode==0 and b'test result: ok. 1 passed' in (root/'test.log').read_bytes()
        results.append({'check':name,'passed':passed,'elapsed_seconds':round(time.monotonic()-started,2)})
        (root/'checks.json').write_text(json.dumps(results,indent=2)+'\n')
        if not passed: raise ValueError('native acceptance phase failed')
    try:
        start(password); phase('save_and_read','write'); stop()
        files = list((root/'vault-home/.local/share/keyrings').glob('*.keyring'))
        if not files or any(b'URDIA_SYNTHETIC_NATIVE_VAULT_KEY' in path.read_bytes() for path in files):
            raise ValueError('persisted vault bytes failed validation')
        results.append({'check':'synthetic_key_absent_from_persisted_bytes','passed':True})
        start('incorrect-'+password); phase('locked_key_not_returned','locked'); stop()
        start(password); phase('restore_after_restart','read'); phase('delete_twice','delete'); stop()
        start(password); phase('deletion_persists_after_restart','delete'); stop()
        (root/'checks.json').write_text(json.dumps(results,indent=2)+'\n')
    finally: stop()


def source_identity():
    # A source archive has no Git history. Never label a surrounding repository
    # as this source or infer a clean commit when metadata is unavailable.
    if not (ROOT / '.git').exists():
        return None, None
    try:
        top = subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], cwd=ROOT, stderr=subprocess.DEVNULL, timeout=5).decode().strip()
        if Path(top).resolve() != ROOT.resolve():
            return None, None
        commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, stderr=subprocess.DEVNULL, timeout=5).decode().strip()
        dirty = bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, stderr=subprocess.DEVNULL, timeout=5).strip())
        return commit, dirty
    except (OSError, subprocess.SubprocessError):
        return None, None


def main(args):
    if sys.platform!='linux': raise ValueError('Linux native test only')
    for path in (args.daemon,args.secret_tool,args.cargo):
        if not path.is_file() or not os.access(path,os.X_OK): raise ValueError('native test executable unavailable')
    if not shutil.which('dbus-run-session') or not shutil.which('gdbus'): raise ValueError('private D-Bus tools unavailable')
    output=args.output.absolute()
    if output.exists() or output.is_symlink() or not output.parent.is_dir() or output.resolve()==ROOT or ROOT in output.resolve().parents: raise ValueError('new evidence output outside checkout required')
    with tempfile.TemporaryDirectory(prefix='urdia-native-vault-') as raw:
        root=Path(raw)
        for name in ('vault-home','runtime','control'): (root/name).mkdir(mode=0o700)
        wrapper=root/'secret-tool-wrapper'
        library=('export LD_LIBRARY_PATH='+shlex.quote(str(args.library_path))+'\n') if args.library_path else ''
        wrapper.write_text('#!/bin/sh\n'+library+'exec '+shlex.quote(str(args.secret_tool))+' "$@"\n');wrapper.chmod(0o700)
        config=root/'bus.conf'
        config.write_text('<busconfig><type>session</type><listen>unix:path='+escape(str(root/'runtime/bus'))+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        env={'PATH':os.environ.get('PATH','/usr/bin:/bin'),'HOME':str(root/'vault-home'),'XDG_RUNTIME_DIR':str(root/'runtime'),'LANG':'C.UTF-8','CARGO_HOME':os.environ.get('CARGO_HOME',str(Path.home()/'.cargo')),'RUSTUP_HOME':os.environ.get('RUSTUP_HOME',str(Path.home()/'.rustup')),'URDIA_NATIVE_SECRET_TOOL':str(wrapper),'URDIA_NATIVE_VAULT_ROOT':str(root)}
        if args.library_path: env['LD_LIBRARY_PATH']=str(args.library_path)
        command=['dbus-run-session','--config-file',str(config),'--',sys.executable,'-I','-B',str(Path(__file__).resolve()),'--worker',str(root),'--daemon',str(args.daemon),'--cargo',str(args.cargo)]
        with (root/'worker.log').open('wb') as log:
            child=subprocess.Popen(command,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                try: status=child.wait(timeout=540)
                except subprocess.TimeoutExpired: status=124
            finally:
                # Always stop this private group, including leaked activation children.
                try: os.killpg(child.pid,signal.SIGTERM)
                except ProcessLookupError: pass
                if child.poll() is None:
                    try: child.wait(timeout=3)
                    except subprocess.TimeoutExpired: os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=3)
        checks=json.loads((root/'checks.json').read_text()) if (root/'checks.json').exists() else []
        commit, dirty = source_identity()
        report={'schema':'urdia.native-vault-acceptance/v1','result':'pass' if status==0 and len(checks)==6 and all(c['passed'] for c in checks) else 'fail','source_commit':commit,'source_commit_dirty':dirty,'source_identity_scope':'Git checkout' if commit else 'unversioned extracted source; no clean-commit claim','credential_store_sha256':hashlib.sha256((ROOT/'crates/urdia-cli/src/credential_store.rs').read_bytes()).hexdigest(),'checks':checks,'tool_sha256':{name:hashlib.sha256(path.read_bytes()).hexdigest() for name,path in [('secret-tool',args.secret_tool),('gnome-keyring-daemon',args.daemon)]},'scope':'native Linux GNOME Secret Service in private D-Bus/HOME; synthetic key and random nonempty vault password; raw-byte absence is not a cryptographic audit','windows_vault_tested':False,'real_credentials_used':False,'model_calls':0,'system_installation_or_service_change':False,'public_listener_started':False,'temporary_vault_and_logs_removed':True}
        output.parent.mkdir(parents=True,exist_ok=True)
        with output.open('x') as stream: stream.write(json.dumps(report,indent=2)+'\n')
        output.chmod(0o600)
        print('Native vault acceptance '+report['result']+'; sanitized evidence saved.')
        return 0 if report['result']=='pass' else 1


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--secret-tool',type=lambda p:Path(p).resolve(),default=Path('/usr/bin/secret-tool'))
    parser.add_argument('--daemon',type=lambda p:Path(p).resolve(),default=Path('/usr/bin/gnome-keyring-daemon'))
    parser.add_argument('--library-path',type=lambda p:Path(p).resolve())
    parser.add_argument('--cargo',type=lambda p:Path(p).absolute(),default=Path(shutil.which('cargo') or '/missing-cargo'))
    parser.add_argument('--output',type=Path)
    parser.add_argument('--worker',type=Path,help=argparse.SUPPRESS)
    args=parser.parse_args()
    try:
        if args.worker: worker(args);sys.exit(0)
        if args.output is None: raise ValueError('evidence output required')
        sys.exit(main(args))
    except (OSError,ValueError,subprocess.SubprocessError):
        print('Native vault acceptance refused or failed; raw diagnostics omitted.',file=sys.stderr);sys.exit(1)
