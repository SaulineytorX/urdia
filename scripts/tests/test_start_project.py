#!/usr/bin/env python3
"""Offline project-launcher regressions, including a real local conversation."""
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
spec = importlib.util.spec_from_file_location('launcher', ROOT / 'scripts/start-project.py')
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class LauncherTests(unittest.TestCase):
    def test_source_build_wins_over_path_and_explicit_selection_wins_over_both(self):
        with tempfile.TemporaryDirectory() as raw:
            base = Path(raw)
            local, installed, explicit = [base / name for name in ('local', 'installed', 'explicit')]
            for path in (local, installed, explicit):
                path.write_text('#!/bin/sh\nexit 0\n')
                path.chmod(0o755)
            with patch.object(launcher.shutil, 'which', return_value=str(installed)) as lookup:
                self.assertEqual(launcher.executable(None, local), local)
                self.assertEqual(launcher.executable(str(explicit), local), explicit)
                lookup.assert_not_called()
                local.unlink()
                self.assertEqual(launcher.executable(None, local), installed)

    def test_invalid_existing_source_binary_does_not_silently_use_path(self):
        with tempfile.TemporaryDirectory() as raw:
            base = Path(raw)
            local = base / 'urdia'
            local.write_text('not executable')
            with patch.object(launcher.shutil, 'which', return_value='/usr/bin/true') as lookup:
                with self.assertRaisesRegex(ValueError, 'executable unavailable'):
                    launcher.executable(None, local)
                lookup.assert_not_called()
                local.unlink()
                local.symlink_to(base / 'missing')
                with self.assertRaises(FileNotFoundError):
                    launcher.executable(None, local)
                lookup.assert_not_called()
                with self.assertRaises(FileNotFoundError):
                    launcher.executable(str(base / 'missing-explicit'), base / 'missing-local')

    def test_paths_with_spaces_and_shell_characters_remain_argv(self):
        with tempfile.TemporaryDirectory() as raw:
            workspace = Path(raw) / 'project $() with spaces'
            workspace.mkdir()
            self.assertEqual(launcher.workspace_path(str(workspace)), workspace)
            args = launcher.command(Path('/usr/bin/urdia'), workspace, Path(raw)/'data', 'qwen')
            self.assertEqual(args[args.index('--workspace')+1], str(workspace))
            self.assertEqual(args[args.index('--bind')+1], '127.0.0.1:0')
            self.assertNotIn('--trusted-session', args)
            self.assertNotIn('--persist-content', args)
            with self.assertRaisesRegex(ValueError, 'WSL path'):
                launcher.workspace_path('C:\\Users\\project')

    def test_data_is_outside_workspace_and_symlinks_are_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            base = Path(raw)
            workspace = base / 'project'
            workspace.mkdir()
            with patch.dict(os.environ, {'XDG_STATE_HOME': str(workspace/'state')}):
                with self.assertRaisesRegex(ValueError, 'outside'):
                    launcher.state_directory(workspace, 'qwen')
            (base/'redirect').symlink_to(workspace, target_is_directory=True)
            with patch.dict(os.environ, {'XDG_STATE_HOME': str(base/'redirect')}):
                with self.assertRaisesRegex(ValueError, 'outside|symlink'):
                    launcher.state_directory(workspace, 'qwen')
            with patch.dict(os.environ, {'XDG_STATE_HOME': str(base/'state')}):
                a = launcher.state_directory(workspace, 'qwen')
                b = launcher.state_directory(workspace, 'preview-demo')
                self.assertNotEqual(a, b)
                self.assertFalse(a.exists())
                launcher.protect_directory(a)
                self.assertEqual(a.stat().st_mode & 0o777, 0o700)
                (a/'events.sqlite').symlink_to(workspace/'database')
                with self.assertRaisesRegex(ValueError, 'symlink'):
                    launcher.protect_directory(a)

    def test_provider_preflight_fails_closed_without_echoing_stderr(self):
        bad = subprocess.CompletedProcess([], 1, b'', b'PRIVATE_CONFIG')
        with patch.object(launcher.subprocess, 'run', return_value=bad):
            with self.assertRaisesRegex(ValueError, 'inspection failed') as failure:
                launcher.available(Path('/urdia'), None, 'qwen')
            self.assertNotIn('PRIVATE_CONFIG', str(failure.exception))
        listing = b'ID\tSTATUS\tAUTH\tINFERENCE\tTRANSPORT\tNAME\nqwen\tunavailable\tunknown\tunknown\tacp\tQwen\n'
        with patch.object(launcher.subprocess, 'run', return_value=subprocess.CompletedProcess([],0,listing)):
            with self.assertRaisesRegex(ValueError, 'unavailable'):
                launcher.available(Path('/urdia'), None, 'qwen')

    def test_profile_catalog_selection_is_bounded_and_redacted(self):
        entry = {'profile': {'id': 'backend', 'provider': 'backend-launch'},
                 'default': True, 'executable_available': True}
        def response(entries):
            return subprocess.CompletedProcess([], 0, json.dumps(entries).encode())
        with patch.object(launcher.subprocess, 'run', return_value=response([entry])):
            self.assertEqual(launcher.selected_profile(Path('/urdia'), Path('/config')), ('backend', 'backend-launch'))
            self.assertEqual(launcher.selected_profile(Path('/urdia'), Path('/config'), 'backend'), ('backend', 'backend-launch'))
            with self.assertRaises(ValueError):
                launcher.selected_profile(Path('/urdia'), Path('/config'), 'missing')
        for entries in [[], [dict(entry, executable_available=False)], [entry, entry], 'invalid',
                        [{'profile': {'id': 'SECRET_SENTINEL'}, 'default': True}]]:
            with patch.object(launcher.subprocess, 'run', return_value=response(entries)):
                if entries == []:
                    self.assertIsNone(launcher.selected_profile(Path('/urdia'), Path('/config')))
                else:
                    with self.assertRaises(ValueError) as failure:
                        launcher.selected_profile(Path('/urdia'), Path('/config'))
                    self.assertNotIn('SECRET_SENTINEL', str(failure.exception))
        with self.assertRaisesRegex(ValueError, 'requires --config'):
            launcher.selected_profile(Path('/urdia'), None, 'backend')
        args = launcher.command(Path('/urdia'), Path('/project'), Path('/data'), 'backend-launch', Path('/config'), 'backend')
        self.assertNotIn('--provider', args)
        self.assertEqual(args[args.index('--profile')+1], 'backend')

    def test_profile_store_is_forwarded_as_argv_without_source_parsing_or_creation(self):
        entry = {'profile': {'id': 'backend', 'provider': 'backend-launch'}, 'default': True, 'executable_available': True}
        store=Path('/private path/$() profiles.toml')
        with patch.object(launcher.subprocess,'run',return_value=subprocess.CompletedProcess([],0,json.dumps([entry]).encode())) as run:
            self.assertEqual(launcher.selected_profile(Path('/urdia'),None,None,store),('backend','backend-launch'))
            self.assertEqual(run.call_args.args[0],['/urdia','--profile-store',str(store),'profiles','--json'])
        listing=b'ID\tSTATUS\tAUTH\tINFERENCE\tTRANSPORT\tNAME\nbackend-launch\tavailable\tunknown\tunknown\tacp\tReference\n'
        with patch.object(launcher.subprocess,'run',return_value=subprocess.CompletedProcess([],0,listing)) as run:
            launcher.available(Path('/urdia'),Path('/config'),'backend-launch',store)
            self.assertEqual(run.call_args.args[0],['/urdia','--config','/config','--profile-store',str(store),'providers'])

    def test_agent_store_forwarding_preserves_literal_argv(self):
        store=Path('/private path/$() agents.toml')
        with patch.object(launcher.subprocess,'run',return_value=subprocess.CompletedProcess([],0,b'[]')) as run:
            self.assertIsNone(launcher.selected_profile(Path('/urdia'),None,None,None,store))
            self.assertEqual(run.call_args.args[0],['/urdia','--agent-store',str(store),'profiles','--json'])
        listing=b'ID\tSTATUS\tAUTH\tINFERENCE\tTRANSPORT\tNAME\nnew-agent\tavailable\tunknown\tunknown\tacp\tReference\n'
        with patch.object(launcher.subprocess,'run',return_value=subprocess.CompletedProcess([],0,listing)) as run:
            launcher.available(Path('/urdia'),None,'new-agent',None,store)
            self.assertEqual(run.call_args.args[0],['/urdia','--agent-store',str(store),'providers'])

    @unittest.skipUnless(os.environ.get('URDIA_TEST_BINARY'), 'live binaries supplied by Cargo wrapper')
    def test_real_profile_preflight_uses_default_or_explicit_selection_without_writes(self):
        with tempfile.TemporaryDirectory(prefix='urdia-profile-launch-test-') as raw:
            base = Path(raw); workspace = base/'project'; workspace.mkdir()
            config = base/'profiles.toml'
            config.write_text('default_profile = "backend"\n' + ''.join(
                '\n[[provider]]\nid = "' + name + '-launch"\nname = "Offline agent"\ncommand = ' +
                json.dumps(os.environ['URDIA_TEST_FAKE']) + '\nargs = ["--stream"]\n' +
                '\n[[profile]]\nid = "' + name + '"\nname = "Offline profile"\nprovider = "' + name +
                '-launch"\nagent = "Reference ACP agent"\ninference_provider = "offline"\nmodel = "none"\n'
                for name in ['backend', 'frontend']))
            argv = [sys_executable(), '-I', '-B', str(ROOT/'scripts/start-project.py'), '--workspace', str(workspace),
                    '--urdia', os.environ['URDIA_TEST_BINARY'], '--config', str(config), '--check']
            env = {'PATH': '/usr/bin:/bin', 'HOME': str(base/'home'), 'XDG_STATE_HOME': str(base/'state')}
            for flags, expected in [([], 'backend'), (['--profile', 'frontend'], 'frontend')]:
                result = subprocess.run([*argv, *flags], env=env, capture_output=True, timeout=15)
                self.assertEqual(result.returncode, 0, 'profile readiness failed')
                self.assertIn(('Profile: ' + expected).encode(), result.stdout)
                self.assertFalse((base/'state').exists())
            result = subprocess.run([*argv, '--profile', 'missing'], env=env, capture_output=True, timeout=15)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((base/'state').exists())

    @unittest.skipUnless(os.environ.get('URDIA_TEST_BINARY'), 'live binaries supplied by Cargo wrapper')
    def test_live_demo_two_turns_authentication_privacy_and_cleanup(self):
        binary = os.environ['URDIA_TEST_BINARY']
        fake = os.environ['URDIA_TEST_FAKE']
        with tempfile.TemporaryDirectory(prefix='urdia-project-preview-test-') as raw:
            base = Path(raw)
            workspace = base/'project with spaces'
            workspace.mkdir()
            (workspace/'keep.txt').write_text('unchanged')
            source = base / 'source without git'
            (source / 'scripts').mkdir(parents=True)
            (source / 'scripts/start-project.py').write_bytes((ROOT/'scripts/start-project.py').read_bytes())
            release = source / 'target/release'
            release.mkdir(parents=True)
            (release / 'urdia').symlink_to(Path(binary).resolve())
            (release / 'urdia-fake-agent').symlink_to(Path(fake).resolve())
            old_bin = base / 'old-installed-version'
            old_bin.mkdir()
            for name in ('urdia', 'urdia-fake-agent'):
                path = old_bin / name
                path.write_text('#!/bin/sh\nexit 99\n')
                path.chmod(0o755)
            self.assertFalse((source / '.git').exists())
            env = {'PATH': str(old_bin)+':/usr/bin:/bin', 'HOME': str(base/'home'),
                   'XDG_STATE_HOME': str(base/'state'), 'LC_ALL': 'C'}
            argv = [sys_executable(), '-I', '-B', str(source/'scripts/start-project.py'),
                    '--workspace', str(workspace), '--demo', '--advisory-ui']
            checked = subprocess.run([*argv, '--check'], env=env, capture_output=True, timeout=15)
            self.assertEqual(checked.returncode, 0, 'demo readiness failed')
            self.assertFalse((base/'state').exists())
            with tempfile.TemporaryFile() as errors:
                process = subprocess.Popen(argv, env=env, stdout=subprocess.PIPE, stderr=errors,
                                           start_new_session=True)
                try:
                    ready = b''
                    deadline = time.monotonic()+15
                    with selectors.DefaultSelector() as selector:
                        selector.register(process.stdout, selectors.EVENT_READ)
                        match = None
                        while time.monotonic() < deadline and not match:
                            for key, _ in selector.select(timeout=0.1):
                                ready += os.read(key.fileobj.fileno(), 4096)
                            match = re.search(rb'(http://127\.0\.0\.1:\d+)/#token=([^\s]+)', ready)
                        self.assertIsNotNone(match, 'local preview did not become ready')
                    url, token = (part.decode() for part in match.groups())
                    def request(path, body=None, authenticated=True):
                        data = json.dumps(body).encode() if body is not None else None
                        headers = {'Authorization': 'Bearer '+token} if authenticated else {}
                        if data: headers['Content-Type'] = 'application/json'
                        with urllib.request.urlopen(urllib.request.Request(url+path,data=data,headers=headers),timeout=3) as response:
                            return response.status, response.read()
                    with self.assertRaises(urllib.error.HTTPError) as failure:
                        request('/api/conversation', authenticated=False)
                    self.assertEqual(failure.exception.code, 401)
                    failure.exception.close()
                    advice_status, advice_body = request('/api/advisory')
                    self.assertEqual(advice_status, 200)
                    advice = json.loads(advice_body)
                    self.assertIsNone(advice['latest'])
                    configured = {item['id']: item['configured'] for item in advice['engines']}
                    self.assertTrue(configured['deterministic'])
                    self.assertFalse(configured['jev'])
                    for text in ('PREVIEW_PRIVATE_ONE', 'PREVIEW_PRIVATE_TWO'):
                        turn = str(uuid.uuid4())
                        status, _ = request('/api/conversation/prompt', {'turn_id':turn,'text':text})
                        self.assertEqual(status, 202)
                        deadline = time.monotonic()+10
                        completed = None
                        while time.monotonic() < deadline:
                            snapshot = json.loads(request('/api/conversation')[1])
                            completed = next((t for t in snapshot['turns'] if t['turn_id']==turn and t['status']=='completed'),None)
                            if completed: break
                            time.sleep(0.03)
                        self.assertIsNotNone(completed, 'demo turn did not complete')
                        self.assertEqual(completed['response'], 'fake-agent: '+text)
                    directory = next((base/'state/urdia/projects').iterdir())
                    with sqlite3.connect(directory/'events.sqlite') as db:
                        dump = '\n'.join(db.iterdump())
                        self.assertNotIn('PREVIEW_PRIVATE_ONE', dump)
                        self.assertNotIn('PREVIEW_PRIVATE_TWO', dump)
                    self.assertEqual(list(workspace.iterdir()), [workspace/'keep.txt'])
                    self.assertEqual((workspace/'keep.txt').read_text(), 'unchanged')
                finally:
                    if process.poll() is None:
                        os.killpg(process.pid, signal.SIGINT)
                        try: process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            os.killpg(process.pid, signal.SIGKILL)
                            process.wait(timeout=5)
                    process.stdout.close()


def sys_executable():
    import sys
    return sys.executable


if __name__ == '__main__':
    unittest.main(verbosity=2)
