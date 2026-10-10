#!/usr/bin/env python3
"""Offline coordinator preflight, isolation, and failure regressions; no builds."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('prepare', ROOT / 'scripts/prepare-core-candidate.py')
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class PreparationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='urdia-prepare-test-')
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.output = self.base / 'output'

    def test_environment_excludes_provider_and_git_credentials(self):
        with patch.dict('os.environ', {'TYPESAFE_API_KEY': 'sentinel', 'GITHUB_TOKEN': 'sentinel',
                                      'SSH_AUTH_SOCK': 'sentinel', 'CARGO_HOME': '/untrusted/cache'}):
            env = prepare.environment(self.base)
        for name in ('TYPESAFE_API_KEY', 'GITHUB_TOKEN', 'SSH_AUTH_SOCK', 'CREDENTIALS_DIRECTORY'):
            self.assertNotIn(name, env)
        self.assertEqual(env['CARGO_HOME'], str(self.base / 'cargo'))
        self.assertEqual(env['GIT_CONFIG_GLOBAL'], '/dev/null')
        self.assertEqual((self.base / 'home').stat().st_mode & 0o777, 0o700)

    def test_invalid_commit_and_existing_output_do_not_start_commands(self):
        with patch.object(prepare.package, 'bounded') as command:
            with self.assertRaisesRegex(ValueError, 'exact lowercase'):
                prepare.prepare(self.output, '../not-a-commit')
            self.output.mkdir()
            with self.assertRaisesRegex(ValueError, 'new output'):
                prepare.prepare(self.output, 'a' * 40)
            command.assert_not_called()

    def test_wrong_commit_refuses_output_creation(self):
        with patch.object(prepare.package, 'bounded', return_value='b' * 40), \
                patch.object(prepare.signal, 'signal'):
            with self.assertRaisesRegex(ValueError, 'differs from checkout'):
                prepare.prepare(self.output, 'a' * 40)
        self.assertFalse(self.output.exists())

    def test_dirty_checkout_refuses_output_creation(self):
        with patch.object(prepare.package, 'bounded', side_effect=['a' * 40, ' M source']), \
                patch.object(prepare.signal, 'signal'):
            with self.assertRaisesRegex(ValueError, 'clean checkout'):
                prepare.prepare(self.output, 'a' * 40)
        self.assertFalse(self.output.exists())

    def test_failed_build_records_failure_and_removes_owned_work(self):
        work_directories = []
        def command(args, env, work, timeout):
            work_directories.append(work)
            if args[:2] == ['git', 'rev-parse']:
                return 'a' * 40
            if args[:2] == ['git', 'status']:
                return ''
            if any(str(arg).endswith('package-core.py') for arg in args):
                raise ValueError('simulated compiler failure')
            return ''
        with patch.object(prepare.package, 'bounded', side_effect=command), \
                patch.object(prepare.package, 'checkout_fingerprint', return_value='c' * 64), \
                patch.object(prepare.signal, 'signal'):
            with self.assertRaisesRegex(ValueError, 'simulated compiler failure'):
                prepare.prepare(self.output, 'a' * 40)
        report = json.loads((self.output / 'preparation-evidence.json').read_bytes())
        self.assertEqual(report['result'], 'fail')
        self.assertEqual(report['failed_stage'], 'build-package-1')
        self.assertFalse(report['publication_allowed'])
        self.assertNotIn('simulated compiler failure', json.dumps(report))
        self.assertEqual(self.output.stat().st_mode & 0o777, 0o700)
        self.assertTrue(all(not directory.exists() for directory in work_directories))


if __name__ == '__main__':
    unittest.main(verbosity=2)
