#!/usr/bin/env python3
"""Bounded, explicit stdlib unittest discovery. Project code runs as the user."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import stat
import sys
import tempfile

spec = importlib.util.spec_from_file_location('urdia_fixed_checks', Path(__file__).with_name('verify-calculator.py'))
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
SKIP = {'.git', '.venv', 'venv', '__pycache__', 'node_modules'}
PROGRAM = '''import sys, unittest
from pathlib import Path
sys.path.insert(0, sys.argv[1])
class ScopedLoader(unittest.TestLoader):
    def _find_test_path(self, full_path, pattern):
        name = Path(full_path).name
        if name.startswith('.') or name in {'.git', '.venv', 'venv', '__pycache__', 'node_modules'}:
            return None, False
        return super()._find_test_path(full_path, pattern)
suite = ScopedLoader().discover(sys.argv[1], pattern="test*.py", top_level_dir=sys.argv[1])
r = unittest.TextTestRunner().run(suite)
raise SystemExit(0 if r.wasSuccessful() and r.testsRun > 0 and not r.skipped and not r.expectedFailures else 1)
'''


def fingerprint(workspace):
    files = []
    entries = 0
    total = 0
    def walk_error(error):
        raise error
    for directory, dirs, names in os.walk(workspace, followlinks=False, onerror=walk_error):
        depth = len(Path(directory).relative_to(workspace).parts)
        if depth > 8:
            raise ValueError('source depth limit')
        entries += len(dirs) + len(names)
        if entries > 2048:
            raise ValueError('entry limit')
        dirs[:] = sorted(d for d in dirs if d not in SKIP and not d.startswith('.'))
        if any(Path(directory, d).is_symlink() for d in dirs):
            raise ValueError('source directory link')
        for name in sorted(names):
            if not name.endswith('.py') or name.startswith('.'):
                continue
            path = Path(directory, name)
            fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
            try:
                info = os.fstat(fd)
                if not stat.S_ISREG(info.st_mode) or info.st_size > 1024 * 1024:
                    raise ValueError('source file limit')
                with os.fdopen(fd, 'rb', closefd=False) as stream:
                    content = stream.read(1024 * 1024 + 1)
                total += len(content)
                if len(content) > 1024 * 1024 or total > 8 * 1024 * 1024:
                    raise ValueError('source bytes limit')
                files.append((path.relative_to(workspace).as_posix(), hashlib.sha256(content).hexdigest()))
                if len(files) > 128:
                    raise ValueError('source count limit')
            finally:
                os.close(fd)
    if not files:
        raise ValueError('no Python sources')
    encoded = json.dumps(sorted(files), ensure_ascii=True, separators=(',', ':')).encode()
    return len(files), hashlib.sha256(encoded).hexdigest()


def verify(workspace, timeout=10):
    count, before = fingerprint(workspace)
    with tempfile.TemporaryDirectory(prefix='urdia-python-check-') as home:
        check = base.run_check(workspace, PROGRAM, timeout, home)
    try:
        unchanged = (count, before) == fingerprint(workspace)
    except (OSError, ValueError):
        unchanged = False
    return {'schema_version': 3, 'contract': 'python-unittest-v1',
            'status': 'passed' if unchanged and check['status'] == 'passed' else 'failed',
            'source_file_count': count, 'files_sha256_before': {'python_sources': before},
            'acceptance_files_unchanged': unchanged, 'checks': {'project_tests': check}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workspace', required=True)
    parser.add_argument('--contract', choices=['python-unittest-v1'], required=True)
    parser.add_argument('--confirm-execution', action='store_true')
    parser.add_argument('--timeout', type=int, default=10)
    args = parser.parse_args()
    if not args.confirm_execution:
        print('Project test execution requires explicit confirmation.', file=sys.stderr)
        return 2
    def interrupt(_signum, _frame):
        base.INTERRUPTED = True
    for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(signum, interrupt)
    try:
        if sys.platform != 'linux' or not 1 <= args.timeout <= 30:
            raise ValueError('unsupported environment')
        workspace = Path(args.workspace).resolve(strict=True)
        if not workspace.is_dir():
            raise ValueError('workspace must be a directory')
        report = verify(workspace, args.timeout)
        if base.INTERRUPTED:
            raise KeyboardInterrupt
    except (OSError, ValueError):
        print('No usable result: review Python test files, source limits and Linux/WSL support.', file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        return 130
    print(json.dumps(report, separators=(',', ':')))
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
