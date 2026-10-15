#!/usr/bin/env python3
"""Check a fixed sample workspace. Executes Python as your user; not a sandbox."""
import argparse
import ast
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

CASES = json.loads(Path(__file__).with_name('cases.json').read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def screen(source):
    """Conservative reproducibility screen, not a security boundary."""
    if len(source.encode()) > 32768:
        raise ValueError('source_too_large')
    tree = ast.parse(source)
    if not tree.body or any(not isinstance(n, ast.FunctionDef) for n in tree.body):
        raise ValueError('functions_only')
    nodes = list(ast.walk(tree))
    if len(nodes) > 2000:
        raise ValueError('source_too_complex')
    calls = {n.name for n in nodes if isinstance(n, ast.FunctionDef)} | {
        'isinstance', 'type', 'str', 'int', 'bool', 'list', 'tuple', 'dict', 'set',
        'sorted', 'len', 'range', 'enumerate', 'zip', 'any', 'all', 'min', 'max',
        'sum', 'TypeError', 'ValueError',
    }
    methods = {'items', 'keys', 'values', 'get', 'copy', 'append', 'extend',
               'sort', 'add', 'remove', 'discard', 'pop', 'update', 'intersection'}
    for node in nodes:
        if isinstance(node, (ast.Import, ast.ImportFrom, ast.ClassDef, ast.Global,
                             ast.Nonlocal, ast.AsyncFunctionDef)):
            raise ValueError('prohibited_construct')
        if isinstance(node, ast.FunctionDef) and (
                node.decorator_list or node.args.defaults or node.args.kw_defaults):
            raise ValueError('decorators_or_defaults')
        if isinstance(node, ast.Name) and node.id.startswith('__'):
            raise ValueError('private_name')
        if isinstance(node, ast.Attribute) and node.attr not in methods:
            raise ValueError('prohibited_attribute')
        if isinstance(node, ast.Call):
            if isinstance(node.func, ast.Name) and node.func.id in calls:
                continue
            if isinstance(node.func, ast.Attribute) and node.func.attr in methods:
                continue
            raise ValueError('prohibited_call')


def check(case, workspace):
    workspace = workspace.resolve(strict=True)
    expected = {'solution.py', 'test_contract.py'}
    if {p.name for p in workspace.iterdir()} != expected:
        raise ValueError('workspace_files')
    for name in expected:
        path = workspace / name
        if not path.is_file() or path.is_symlink():
            raise ValueError('regular_files_required')
    source, tests = workspace / 'solution.py', workspace / 'test_contract.py'
    if tests.read_bytes() != case['tests'].encode():
        raise ValueError('protected_tests_changed')
    screen(source.read_text())
    before = {p: digest(workspace / p) for p in expected}
    # The trusted checker bootstraps imports under isolated Python. Test data and
    # property checks live outside the agent workspace; they are public fixtures.
    bootstrap = '''import os,resource,sys
resource.setrlimit(resource.RLIMIT_CPU,(2,2))
resource.setrlimit(resource.RLIMIT_AS,(268435456,268435456))
sys.path.insert(0,os.getcwd())
'''
    project = '''import unittest
suite=unittest.defaultTestLoader.discover('.',pattern='test_contract.py')
r=unittest.TextTestRunner().run(suite)
sys.exit(0 if r.wasSuccessful() and r.testsRun==8 else 1)
'''
    results = {}
    for name, code in [('project', project), ('properties', case['extra'])]:
        try:
            run = subprocess.run([sys.executable, '-I', '-B', '-c', bootstrap + code],
                                 cwd=workspace, env={'PATH': '/usr/bin:/bin'},
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                 timeout=5, check=False)
            results[name] = run.returncode == 0
        except subprocess.TimeoutExpired:
            results[name] = False
        if any(digest(workspace / p) != before[p] for p in expected):
            raise ValueError('files_changed_during_checks')
    return dict(case=case['id'], passed=all(results.values()),
                project_tests=8, property_assertions=case['extra_assertions'],
                protected_tests_unchanged=True, source_sha256=before['solution.py'],
                checks=results)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('case', choices=[c['id'] for c in CASES])
    parser.add_argument('workspace', type=Path)
    args = parser.parse_args()
    try:
        result = check(next(c for c in CASES if c['id'] == args.case), args.workspace)
    except (ValueError, OSError, SyntaxError, UnicodeError) as error:
        result = {'passed': False, 'error_kind': type(error).__name__}
    print(json.dumps(result, sort_keys=True))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
