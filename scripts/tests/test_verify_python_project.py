#!/usr/bin/env python3
"""Offline general-project acceptance; private project output is never evidence."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'scripts/verify-python-project.py'
spec = importlib.util.spec_from_file_location('project_checker', SCRIPT)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)

class ProjectChecks(unittest.TestCase):
    def run_project(self, project, confirmed=True, timeout=2):
        return subprocess.run([sys.executable, '-I', '-B', str(SCRIPT), '--workspace', str(project),
                               '--contract', 'python-unittest-v1', '--timeout', str(timeout)]
                              + (['--confirm-execution'] if confirmed else []),
                              capture_output=True, text=True, timeout=12)
    def test_real_project_pass_omits_logs_names_and_paths(self):
        with tempfile.TemporaryDirectory() as raw:
            project = Path(raw)
            (project/'tax.py').write_text('def tax(amount): return amount * 0.16\n')
            (project/'test_tax.py').write_text('import unittest\nfrom tax import tax\nprint("PRIVATE_OUTPUT")\nclass T(unittest.TestCase):\n def test_private_name(self): self.assertAlmostEqual(tax(100),16)\n')
            result = self.run_project(project)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report['source_file_count'], 2)
            self.assertEqual(report['checks']['project_tests']['unittest']['tests_run'], 1)
            for secret in ('PRIVATE_OUTPUT', 'test_private_name', raw, 'test_tax.py'):
                self.assertNotIn(secret, result.stdout+result.stderr)
    def test_fail_empty_skip_import_error_and_mutation_never_pass(self):
        examples = [
            'import unittest\nclass T(unittest.TestCase):\n def test_x(self): self.fail("PRIVATE_FAILURE")\n',
            'import unittest\n',
            'import unittest\nclass T(unittest.TestCase):\n @unittest.skip("PRIVATE_SKIP")\n def test_x(self): pass\n',
            'import missing_private_dependency\n',
            'import unittest\nclass T(unittest.TestCase):\n @unittest.expectedFailure\n def test_x(self): self.fail("PRIVATE_EXPECTED")\n',
            'import unittest\nfrom pathlib import Path\nclass T(unittest.TestCase):\n def test_x(self): Path("added.py").write_text("x=1")\n',
        ]
        for source in examples:
            with self.subTest(source=source), tempfile.TemporaryDirectory() as raw:
                project=Path(raw);(project/'test_project.py').write_text(source)
                result=self.run_project(project)
                self.assertEqual(result.returncode,1,result.stderr)
                self.assertEqual(json.loads(result.stdout)['status'],'failed')
                self.assertNotIn('PRIVATE_',result.stdout+result.stderr)
    def test_execution_requires_consent_and_timeout_is_bounded(self):
        with tempfile.TemporaryDirectory() as raw:
            project=Path(raw);(project/'test_project.py').write_text('from pathlib import Path\nPath("side_effect").touch()\n')
            self.assertEqual(self.run_project(project,False).returncode,2)
            self.assertFalse((project/'side_effect').exists())
            (project/'test_project.py').write_text('import time\ntime.sleep(60)\n')
            result=self.run_project(project,timeout=1)
            self.assertEqual(result.returncode,1)
            self.assertEqual(json.loads(result.stdout)['checks']['project_tests']['status'],'timed_out')
    def test_links_source_limits_and_ignored_packages(self):
        with tempfile.TemporaryDirectory() as raw:
            project=Path(raw);(project/'test_project.py').write_text('import unittest\nclass T(unittest.TestCase):\n def test_x(self): pass\n')
            excluded=project/'node_modules';excluded.mkdir();(excluded/'__init__.py').write_text('')
            (excluded/'test_fail.py').write_text('raise RuntimeError("PRIVATE_IGNORED")\n')
            self.assertEqual(self.run_project(project).returncode,0)
            link=project/'link.py';link.symlink_to(project/'test_project.py')
            self.assertEqual(self.run_project(project).returncode,2);link.unlink()
            for i in range(128): (project/f'module_{i}.py').write_text('')
            with self.assertRaises(ValueError): checker.fingerprint(project)

if __name__ == '__main__': unittest.main()
