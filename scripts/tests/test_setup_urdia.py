"""Offline wizard boundaries and real authenticated demo from a Git-free source layout."""
import importlib.util
import json
import os
from pathlib import Path
import re
import selectors
import signal
import sqlite3
import subprocess
import tempfile
import time
import unittest
import urllib.error
import urllib.request
import uuid
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('wizard', ROOT / 'scripts/setup-urdia.py')
wizard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wizard)


class SetupTests(unittest.TestCase):
    def test_declined_launch_creates_no_session_settings_and_never_executes(self):
        with tempfile.TemporaryDirectory() as raw:
            project = Path(raw)
            with patch.object(wizard.launcher, 'executable', return_value=Path('/trusted/binary')), patch('sys.argv', ['setup', '--no-build', '--demo', '--workspace', str(project)]), patch('builtins.input', return_value='n'), patch.object(wizard, 'launch_arguments') as launch, patch.object(wizard.os, 'execv') as execute:
                self.assertEqual(wizard.main(), 0)
                launch.assert_not_called()
                execute.assert_not_called()
            self.assertEqual(list(project.iterdir()), [])

    def test_prerequisite_check_never_builds_discovers_accounts_or_writes(self):
        with patch('sys.argv', ['setup', '--check']), patch.object(wizard, 'prerequisites', return_value=[]), patch.object(wizard, 'build') as build, patch.object(wizard, 'choose_agent') as choose, patch.object(wizard.os, 'execv') as execute:
            self.assertEqual(wizard.main(), 0)
            build.assert_not_called();choose.assert_not_called();execute.assert_not_called()

    def test_incompatible_codex_is_not_offered_or_silently_downgraded(self):
        def which(name):
            return '/trusted/codex' if name == 'codex' else None
        with patch.object(wizard.shutil, 'which', side_effect=which), patch.object(wizard.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, b'codex-cli 0.160.0')), patch('builtins.input', return_value='2'):
            with self.assertRaises(ValueError): wizard.choose_agent()
        with patch.object(wizard.shutil, 'which', side_effect=which), patch.object(wizard.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, b'codex-cli 0.159.2')), patch('builtins.input', return_value='2'):
            self.assertEqual(wizard.choose_agent(), 'codex')

    def test_invalid_workspace_is_refused_before_build(self):
        with patch('sys.argv', ['setup', '--demo', '--workspace', 'C:\\project']), patch.object(wizard, 'prerequisites', return_value=[]), patch.object(wizard, 'build') as build:
            with self.assertRaises(ValueError): wizard.main()
            build.assert_not_called()

    def test_codex_config_is_private_key_free_and_never_overwrites_or_follows_links(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw);workspace = root / 'Project space';workspace.mkdir();state = root / 'state'
            with patch.dict(os.environ, {'XDG_STATE_HOME':str(state)}), patch.object(wizard.shutil, 'which', return_value='/trusted/Codex space/codex'):
                args = wizard.launch_arguments(Path('/trusted/urdia'), workspace, 'codex')
                config = Path(args[args.index('--config')+1]);data = config.read_bytes()
                self.assertFalse(config.is_relative_to(workspace))
                self.assertEqual(config.stat().st_mode & 0o777, 0o600)
                self.assertIn(b'/trusted/Codex space/codex', data)
                self.assertNotIn(b'API_KEY', data)
                self.assertIn('--agent-store',args);self.assertIn('--profile-store',args)
                wizard.launch_arguments(Path('/trusted/urdia'), workspace, 'codex')
                config.write_bytes(b'operator edit')
                with self.assertRaises(ValueError):wizard.launch_arguments(Path('/trusted/urdia'),workspace,'codex')
                self.assertEqual(config.read_bytes(),b'operator edit')
                config.unlink();target=root/'private';target.write_bytes(b'do not touch');config.symlink_to(target)
                with self.assertRaises(OSError):wizard.launch_arguments(Path('/trusted/urdia'),workspace,'codex')
                self.assertEqual(target.read_bytes(),b'do not touch')

    def test_registered_codex_is_reused_without_a_duplicate_bootstrap_config(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw); workspace = root / 'project'; workspace.mkdir()
            with patch.dict(os.environ, {'XDG_STATE_HOME':str(root / 'state')}):
                settings = wizard.launcher.state_directory(workspace, 'guided-settings')
                wizard.launcher.protect_directory(settings)
                saved = settings / 'agents.toml'; saved.write_text('operator catalog'); saved.chmod(0o600)
                with patch.object(wizard.launcher, 'available') as inspect:
                    args = wizard.launch_arguments(Path('/trusted/binary'),workspace,'codex')
                    inspect.assert_called_once_with(Path('/trusted/binary'),None,'codex',agent_store=saved)
                self.assertNotIn('--config',args)
                self.assertFalse((settings / 'codex-provider.toml').exists())
                self.assertEqual(saved.read_text(),'operator catalog')

    @unittest.skipUnless(os.environ.get('URDIA_TEST_BINARY'), 'Cargo supplies offline binaries')
    def test_real_demo_launch_from_git_free_layout_has_no_project_edits_or_content_retention(self):
        with tempfile.TemporaryDirectory() as raw:
            root=Path(raw);source=root/'source';(source/'scripts').mkdir(parents=True)
            for name in ('setup-urdia.py','start-project.py'):
                (source/'scripts'/name).write_bytes((ROOT/'scripts'/name).read_bytes())
            workspace=root/'Project space';workspace.mkdir();(workspace/'keep.txt').write_text('unchanged')
            env={'PATH':'/usr/bin:/bin','HOME':str(root),'XDG_STATE_HOME':str(root/'state'),'LC_ALL':'C.UTF-8'}
            p=subprocess.Popen(['/usr/bin/python3','-I','-B',str(source/'scripts/setup-urdia.py'),'--no-build','--demo','--workspace',str(workspace),'--urdia',os.environ['URDIA_TEST_BINARY'],'--fake-agent',os.environ['URDIA_TEST_FAKE']],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,start_new_session=True)
            try:
                p.stdin.write(b'y\n');p.stdin.close();output=b'';deadline=time.monotonic()+15;match=None
                with selectors.DefaultSelector() as selector:
                    selector.register(p.stdout,selectors.EVENT_READ)
                    while time.monotonic()<deadline and not match:
                        for key,_ in selector.select(.1):output+=os.read(key.fileobj.fileno(),4096)
                        match=re.search(rb'http://127\.0\.0\.1:\d+/#token=([^\s]+)',output)
                        if p.poll() is not None:break
                self.assertIsNotNone(match)
                url=match.group().decode();base=url.split('/#')[0];token=match[1].decode()
                with urllib.request.urlopen(urllib.request.Request(base+'/api/conversation',headers={'Authorization':'Bearer '+token}),timeout=5) as response:
                    self.assertEqual(response.status,200)
                with self.assertRaises(urllib.error.HTTPError) as refused:
                    urllib.request.urlopen(base+'/api/conversation',timeout=5)
                self.assertEqual(refused.exception.code,401);refused.exception.close()
                marker='GUIDED_PRIVATE_DEMO_MESSAGE'
                body=json.dumps({'turn_id':str(uuid.uuid4()),'text':marker}).encode()
                with urllib.request.urlopen(urllib.request.Request(base+'/api/conversation/prompt',data=body,headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'}),timeout=5) as response:
                    self.assertEqual(response.status,202)
                deadline=time.monotonic()+5
                while time.monotonic()<deadline:
                    with urllib.request.urlopen(urllib.request.Request(base+'/api/conversation',headers={'Authorization':'Bearer '+token}),timeout=5) as response:
                        snapshot=json.load(response)
                    if snapshot['turns'] and snapshot['turns'][0]['status']=='completed':break
                    time.sleep(.05)
                self.assertEqual(snapshot['turns'][0]['status'],'completed')
                self.assertIn(marker,snapshot['turns'][0]['response'])
                self.assertEqual((workspace/'keep.txt').read_text(),'unchanged')
                self.assertEqual(sorted(x.name for x in workspace.iterdir()),['keep.txt'])
                self.assertFalse((source/'.git').exists())
                databases=list((root/'state').rglob('events.sqlite'));self.assertEqual(len(databases),1)
                with sqlite3.connect(databases[0]) as db:
                    dump='\n'.join(db.iterdump());self.assertNotIn(token,dump);self.assertNotIn(marker,dump)
            finally:
                try:os.killpg(p.pid,signal.SIGTERM)
                except ProcessLookupError:pass
                try:p.wait(timeout=5)
                except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait(timeout=5)


if __name__ == '__main__':unittest.main()
