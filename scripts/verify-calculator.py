#!/usr/bin/env python3
"""Explicit Linux/WSL verification of reviewed calculator acceptance tasks.

Runs workspace Python as the current user, not in a security sandbox. No AI calls.
Raw project output is discarded; only bounded host observations are returned.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import stat
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
FILES = ('calculator.py', 'test_calculator.py')
MAX_FILE_BYTES = 1024 * 1024
INTERRUPTED = False
CONTRACT = '''import sys, unittest
sys.path.insert(0, sys.argv[1])
from calculator import add
class Contract(unittest.TestCase):
    def test_positive(self): self.assertEqual(add(2, 3), 5)
    def test_negative(self): self.assertEqual(add(-2, 3), 1)
    def test_zero(self): self.assertEqual(add(0, 0), 0)
r = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(Contract))
raise SystemExit(0 if r.wasSuccessful() and r.testsRun == 3 else 1)
'''
FIVE_CONTRACT = '''import sys, unittest, math
sys.path.insert(0, sys.argv[1])
from calculator import calculate
class Contract(unittest.TestCase):
    def test_add(self): self.assertEqual(calculate([1, 2, 3, 4, 5], "add"), 15)
    def test_subtract(self): self.assertEqual(calculate([20, 1, 2, 3, 4], "subtract"), 10)
    def test_multiply(self): self.assertEqual(calculate([1, 2, 3, 4, 5], "multiply"), 120)
    def test_divide(self): self.assertAlmostEqual(calculate([120, 2, 3, 2, 5], "divide"), 2)
    def test_fractional_negative(self): self.assertAlmostEqual(calculate([-1.5, 2, 0.5, 0, -3], "add"), -2)
    def test_zero_numerator(self): self.assertEqual(calculate([0, 1, 2, 3, 4], "divide"), 0)
    def test_zero_denominator(self):
        for i in range(1, 5):
            values = [1, 2, 3, 4, 5]; values[i] = 0
            with self.subTest(i=i), self.assertRaises(ZeroDivisionError): calculate(values, "divide")
    def test_count(self):
        for values in ([], [1]*4, [1]*6):
            with self.subTest(values=values), self.assertRaises(ValueError): calculate(values, "add")
    def test_types(self):
        for values in (None, "12345", {1,2,3,4,5}, [True,2,3,4,5], [1,"2",3,4,5], [1,None,3,4,5]):
            with self.subTest(values=values), self.assertRaises(TypeError): calculate(values, "add")
    def test_nonfinite(self):
        for number in (math.inf, -math.inf, math.nan):
            with self.subTest(number=number), self.assertRaises(ValueError): calculate([1,number,3,4,5], "add")
    def test_operation(self):
        for operation in ("sum", "ADD", "", None):
            with self.subTest(operation=operation), self.assertRaises(ValueError): calculate([1,2,3,4,5], operation)
    def test_no_mutation_and_tuple(self):
        for operation in ("add", "subtract", "multiply", "divide"):
            values = [10, 2, 3, 4, 5]; before = values.copy()
            result = calculate(values, operation)
            self.assertEqual(values, before)
            self.assertEqual(calculate(tuple(values), operation), result)
r = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(Contract))
raise SystemExit(0 if r.wasSuccessful() and r.testsRun == 12 else 1)
'''
CONTRACTS = {'calculator-add-v1': CONTRACT, 'calculator-five-v1': FIVE_CONTRACT}
PROJECT_TESTS = '''import sys, unittest
sys.path.insert(0, sys.argv[1])
import test_calculator
suite = unittest.defaultTestLoader.loadTestsFromModule(test_calculator)
r = unittest.TextTestRunner().run(suite)
raise SystemExit(0 if r.wasSuccessful() and r.testsRun > 0 and not r.skipped else 1)
'''


# Each trusted test harness writes only aggregate counts to a separate private file.
# Project stdout/stderr stays discarded; it cannot be parsed as a count report.
COUNTS = ('tests_run', 'failures', 'errors', 'skipped', 'expected_failures', 'unexpected_successes')
SUMMARY = """import json
with open(sys.argv[2], 'w', encoding='utf-8') as output:
    json.dump({'tests_run': r.testsRun, 'failures': len(r.failures), 'errors': len(r.errors),
               'skipped': len(r.skipped), 'expected_failures': len(r.expectedFailures),
               'unexpected_successes': len(r.unexpectedSuccesses)}, output)
"""


def read_counts(path):
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            if not stat.S_ISREG(os.fstat(fd).st_mode):
                return None
            with os.fdopen(fd, 'rb', closefd=False) as stream:
                content = stream.read(1025)
        finally:
            os.close(fd)
        if len(content) > 1024:
            return None
        counts = json.loads(content)
        if (not isinstance(counts, dict) or set(counts) != set(COUNTS)
                or any(type(value) is not int or not 0 <= value <= 1_000_000 for value in counts.values())):
            return None
        return counts
    except (OSError, ValueError, UnicodeError):
        return None


def fingerprint(workspace):
    result = {}
    for name in FILES:
        # Refuse links/non-regular files before opening; O_NONBLOCK avoids FIFO races.
        fd = os.open(workspace / name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_size > MAX_FILE_BYTES:
                raise ValueError('invalid acceptance file')
            with os.fdopen(fd, 'rb', closefd=False) as stream:
                content = stream.read(MAX_FILE_BYTES + 1)
            if len(content) > MAX_FILE_BYTES:
                raise ValueError('acceptance file exceeds limit')
            result[name] = hashlib.sha256(content).hexdigest()
        finally:
            os.close(fd)
    return result


def run_check(workspace, program, timeout, home):
    if INTERRUPTED:
        raise KeyboardInterrupt
    fd, summary = tempfile.mkstemp(prefix='counts-', dir=home)
    os.close(fd)
    program = program.replace('raise SystemExit(', SUMMARY + '\nraise SystemExit(', 1)
    command = [sys.executable, '-I', '-B', str(ROOT / 'scripts/smoke-supervisor.py'),
               '--timeout', str(timeout), '--kill-after', '1',
               '--stdout', '/dev/null', '--stderr', '/dev/null', '--',
               sys.executable, '-I', '-B', '-c', program, str(workspace), summary]
    started = time.monotonic_ns()
    child = subprocess.Popen(command, cwd=workspace, stdin=subprocess.DEVNULL,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                             env={'PATH': '/usr/bin:/bin', 'HOME': home, 'LC_ALL': 'C'},
                             start_new_session=True)
    while True:
        if INTERRUPTED:
            child.send_signal(signal.SIGINT)
            child.wait()
            raise KeyboardInterrupt
        try:
            code = child.wait(timeout=0.1)
            break
        except subprocess.TimeoutExpired:
            continue
    counts = read_counts(summary)
    if code == 0 and (counts is None or counts['tests_run'] == 0
                      or any(counts[key] for key in ('failures', 'errors', 'skipped', 'unexpected_successes'))):
        raise ValueError('missing or inconsistent test counts')
    return {'unittest': counts, 'status': 'passed' if code == 0 else 'timed_out' if code == 124 else 'failed',
            'exit_code': code, 'elapsed_ms': (time.monotonic_ns() - started) // 1_000_000}


def verify(workspace, timeout, contract='calculator-add-v1'):
    before = fingerprint(workspace)
    with tempfile.TemporaryDirectory(prefix='urdia-calculator-check-') as home:
        checks = {'host_contract': run_check(workspace, CONTRACTS[contract], timeout, home),
                  'project_tests': run_check(workspace, PROJECT_TESTS, timeout, home)}
    try:
        unchanged = before == fingerprint(workspace)
    except (OSError, ValueError):
        unchanged = False
    return {'schema_version': 2, 'contract': contract,
            'status': 'passed' if unchanged and all(c['status'] == 'passed' for c in checks.values()) else 'failed',
            'files_sha256_before': before, 'acceptance_files_unchanged': unchanged, 'checks': checks}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workspace', required=True)
    parser.add_argument('--contract', choices=tuple(CONTRACTS), default='calculator-add-v1')
    parser.add_argument('--confirm-execution', action='store_true',
                        help='authorize executing project Python as your user; not a sandbox')
    parser.add_argument('--timeout', type=int, default=10, help='seconds per check, 1–30')
    args = parser.parse_args()
    def interrupt(_signum, _frame):
        global INTERRUPTED
        INTERRUPTED = True
    for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(signum, interrupt)
    if not args.confirm_execution:
        print('Verification requires --confirm-execution; project Python runs as your user.', file=sys.stderr)
        return 2
    try:
        if sys.platform != 'linux' or not 1 <= args.timeout <= 30:
            raise ValueError('unsupported environment or deadline')
        workspace = Path(args.workspace).expanduser().resolve(strict=True)
        if not workspace.is_dir():
            raise ValueError('workspace must be a directory')
        report = verify(workspace, args.timeout, args.contract)
        if INTERRUPTED:
            raise KeyboardInterrupt
    except (OSError, ValueError):
        print('Verification refused: check Linux/WSL, workspace, deadline, the two regular acceptance files and valid test counts.', file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print('Verification interrupted; no passing result.', file=sys.stderr)
        return 130
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
