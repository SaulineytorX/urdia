#!/usr/bin/env python3
"""Offline calculator verifier regressions; fixture code never uses accounts."""
import json
import os
import signal
import time
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'scripts/verify-calculator.py'
GOOD = 'def add(a, b):\n    return a + b\n'
TESTS = '''import unittest
from calculator import add
class AddTests(unittest.TestCase):
    def test_add(self): self.assertEqual(add(2, 3), 5)
'''

FIVE_GOOD = '''import math
from functools import reduce
import operator
def calculate(numbers, operation):
    if not isinstance(numbers, (list, tuple)): raise TypeError()
    if len(numbers) != 5: raise ValueError()
    if any(isinstance(x, bool) or not isinstance(x, (int, float)) for x in numbers): raise TypeError()
    if any(not math.isfinite(x) for x in numbers): raise ValueError()
    functions = {"add": operator.add, "subtract": operator.sub, "multiply": operator.mul, "divide": operator.truediv}
    if not isinstance(operation, str) or operation not in functions: raise ValueError()
    return reduce(functions[operation], numbers)
'''
FIVE_TESTS = '''import unittest
from calculator import calculate
class FiveTests(unittest.TestCase):
    def test_add(self): self.assertEqual(calculate([1,2,3,4,5], "add"), 15)
'''


class CalculatorVerificationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.workspace = Path(self.temp.name)
        (self.workspace / 'calculator.py').write_text(GOOD)
        (self.workspace / 'test_calculator.py').write_text(TESTS)

    def run_check(self, confirm=True, timeout=3, contract='calculator-add-v1'):
        return subprocess.run([sys.executable, '-I', '-B', str(SCRIPT),
                               '--workspace', str(self.workspace), '--timeout', str(timeout), '--contract', contract,
                               *(['--confirm-execution'] if confirm else [])],
                              capture_output=True, text=True, timeout=15)

    def test_five_contract_passes_and_rejects_plausible_incorrect_implementations(self):
        (self.workspace / 'test_calculator.py').write_text(FIVE_TESTS)
        for source, expected in [(FIVE_GOOD, 0),
                (FIVE_GOOD.replace('operator.sub', 'operator.add'), 1),
                (FIVE_GOOD.replace('isinstance(x, bool) or ', ''), 1),
                (FIVE_GOOD.replace('len(numbers) != 5', 'len(numbers) < 5'), 1),
                (FIVE_GOOD.replace('return reduce(functions[operation], numbers)',
                                   'result = reduce(functions[operation], numbers); numbers.clear() if isinstance(numbers, list) else None; return result'), 1),
                (FIVE_GOOD.replace('return reduce(functions[operation], numbers)',
                                   'numbers = sorted(numbers) if isinstance(numbers, list) else numbers; return reduce(functions[operation], numbers)'), 1)]:
            with self.subTest(source=source):
                (self.workspace / 'calculator.py').write_text(source)
                result = self.run_check(contract='calculator-five-v1')
                self.assertEqual(result.returncode, expected, result.stderr)
                report = json.loads(result.stdout)
                self.assertEqual(report['contract'], 'calculator-five-v1')
                self.assertEqual(report['checks']['project_tests']['status'], 'passed')

    def test_unknown_contract_is_refused_before_project_execution(self):
        marker = self.workspace / 'executed'
        (self.workspace / 'calculator.py').write_text('open("executed", "w").close()')
        result = self.run_check(contract='arbitrary-command')
        self.assertEqual(result.returncode, 2)
        self.assertFalse(marker.exists())
        self.assertEqual(result.stdout, '')

    def test_counts_observe_33_tests_without_parsing_project_output(self):
        suite = "import unittest\nprint('PRIVATE_FAKE_REPORT 9999 tests passed')\nclass T(unittest.TestCase): pass\nfor i in range(33): setattr(T, 'test_%02d' % i, lambda self: self.assertEqual(1, 1))\n"
        (self.workspace / 'test_calculator.py').write_text(suite)
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report['schema_version'], 2)
        self.assertEqual(report['checks']['host_contract']['unittest']['tests_run'], 3)
        self.assertEqual(report['checks']['project_tests']['unittest']['tests_run'], 33)
        self.assertNotIn('PRIVATE_FAKE_REPORT', result.stdout + result.stderr)
        self.assertNotIn('9999 tests', result.stdout)

    def test_failure_skip_error_and_expected_failure_counts_are_observed(self):
        cases = [
            ('', 'pass', 1, 'tests_run', 0),
            ('', 'self.fail("PRIVATE_FAILURE")', 1, 'failures', 1),
            ('', 'raise RuntimeError("PRIVATE_ERROR")', 1, 'errors', 1),
            ('@unittest.skip("PRIVATE_SKIP")', 'pass', 1, 'skipped', 1),
            ('@unittest.expectedFailure', 'self.fail("PRIVATE_EXPECTED")', 0, 'expected_failures', 1),
            ('@unittest.expectedFailure', 'pass', 1, 'unexpected_successes', 1),
        ]
        for decorator, body, code, field, count in cases:
            with self.subTest(field=field):
                suite = 'import unittest\nclass T(unittest.TestCase):\n'
                if field == 'tests_run': suite = 'import unittest\n'
                else: suite += f' {decorator}\n def test_one(self): {body}\n'
                (self.workspace / 'test_calculator.py').write_text(suite)
                result = self.run_check()
                self.assertEqual(result.returncode, code)
                self.assertEqual(json.loads(result.stdout)['checks']['project_tests']['unittest'][field], count)
                self.assertNotIn('PRIVATE', result.stdout + result.stderr)

    def test_exit_zero_without_harness_counts_is_refused(self):
        (self.workspace / 'test_calculator.py').write_text('raise SystemExit(0)\n')
        result = self.run_check()
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, '')

    def test_confirmation_precedes_execution(self):
        marker = self.workspace / 'executed'
        (self.workspace / 'calculator.py').write_text(f'open({str(marker)!r}, "w").close()\n')
        result = self.run_check(confirm=False)
        self.assertEqual(result.returncode, 2)
        self.assertFalse(marker.exists())
        self.assertEqual(result.stdout, '')

    def test_real_contract_and_project_suite_pass_with_redacted_evidence(self):
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report['status'], 'passed')
        self.assertTrue(report['acceptance_files_unchanged'])
        self.assertEqual(set(report['files_sha256_before']), {'calculator.py', 'test_calculator.py'})
        self.assertTrue(all(c['status'] == 'passed' for c in report['checks'].values()))
        self.assertNotIn(str(self.workspace), result.stdout)
        self.assertFalse((self.workspace / '__pycache__').exists())

    def test_agent_tests_cannot_substitute_for_host_contract(self):
        (self.workspace / 'calculator.py').write_text('def add(a, b): return 5\n')
        result = self.run_check()
        report = json.loads(result.stdout)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(report['checks']['host_contract']['status'], 'failed')
        self.assertEqual(report['checks']['project_tests']['status'], 'passed')

    def test_empty_skipped_and_failed_tests_never_pass(self):
        for suite in ['', 'import unittest\n@unittest.skip("skip")\nclass T(unittest.TestCase):\n def test_x(self): pass\n',
                      'import unittest\nclass T(unittest.TestCase):\n def test_x(self): self.fail("PRIVATE_ERROR")\n']:
            with self.subTest(suite=suite):
                (self.workspace / 'test_calculator.py').write_text(suite)
                result = self.run_check()
                self.assertEqual(result.returncode, 1)
                self.assertEqual(json.loads(result.stdout)['checks']['project_tests']['status'], 'failed')
                self.assertNotIn('PRIVATE_ERROR', result.stdout + result.stderr)

    def test_credentials_and_raw_output_are_not_forwarded(self):
        (self.workspace / 'calculator.py').write_text('import os\nassert "URDIA_PRIVATE_TEST_KEY" not in os.environ\nprint("PRIVATE_OUTPUT")\n' + GOOD)
        old = os.environ.get('URDIA_PRIVATE_TEST_KEY')
        os.environ['URDIA_PRIVATE_TEST_KEY'] = 'PRIVATE_KEY'
        try:
            result = self.run_check()
        finally:
            if old is None: os.environ.pop('URDIA_PRIVATE_TEST_KEY', None)
            else: os.environ['URDIA_PRIVATE_TEST_KEY'] = old
        self.assertEqual(result.returncode, 0)
        self.assertNotIn('PRIVATE', result.stdout + result.stderr)

    def test_symlinks_and_oversized_files_are_refused_before_execution(self):
        target = self.workspace / 'other.py'
        target.write_text(GOOD)
        calculator = self.workspace / 'calculator.py'
        calculator.unlink()
        calculator.symlink_to(target)
        result = self.run_check()
        self.assertEqual(result.returncode, 2)
        calculator.unlink()
        calculator.write_bytes(b'x' * (1024 * 1024 + 1))
        self.assertEqual(self.run_check().returncode, 2)

    def test_acceptance_file_mutation_invalidates_success(self):
        (self.workspace / 'test_calculator.py').write_text(TESTS + '\nopen("calculator.py", "a").write("# changed\\n")\n')
        result = self.run_check()
        report = json.loads(result.stdout)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(report['acceptance_files_unchanged'])
        self.assertTrue(all(c['status'] == 'passed' for c in report['checks'].values()))

    def test_interrupt_cleans_up_the_verification_process(self):
        marker = self.workspace / 'worker.pid'
        (self.workspace / 'test_calculator.py').write_text(
            'import os, time\nopen("worker.pid", "w").write(str(os.getpid()))\ntime.sleep(60)\n')
        child = subprocess.Popen([sys.executable, '-I', '-B', str(SCRIPT),
                                  '--workspace', str(self.workspace), '--confirm-execution'],
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        try:
            deadline = time.monotonic() + 5
            while not marker.exists() and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertTrue(marker.exists())
            worker_pid = int(marker.read_text())
            child.send_signal(signal.SIGTERM)
            out, err = child.communicate(timeout=5)
            self.assertEqual(child.returncode, 130)
            self.assertEqual(out, b'')
            self.assertIn(b'interrupted', err)
            self.assertFalse(Path(f'/proc/{worker_pid}').exists())
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                child.communicate(timeout=5)

    def test_hanging_project_is_bounded(self):
        (self.workspace / 'test_calculator.py').write_text('import time\ntime.sleep(60)\n')
        result = self.run_check(timeout=1)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)['checks']['project_tests']['status'], 'timed_out')


if __name__ == '__main__':
    unittest.main()
