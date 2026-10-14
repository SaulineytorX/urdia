#!/usr/bin/env python3
"""Explicit checks for the repair example; executes project Python as the caller."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import sys
import unittest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('workspace', type=Path)
    args = parser.parse_args()
    workspace = args.workspace.resolve(strict=True)
    manifest = json.loads(Path(__file__).with_name('fixture.json').read_text())
    tests = workspace / manifest['protected_test_file']
    if not tests.is_file() or tests.is_symlink():
        parser.error('the protected test file must be a regular file')
    if hashlib.sha256(tests.read_bytes()).hexdigest() != manifest['protected_tests_sha256']:
        parser.error('the protected test file changed; this repair cannot pass')
    source = workspace / manifest['source_file']
    if not source.is_file() or source.is_symlink():
        parser.error('normalize.py must be a regular file')
    function = runpy.run_path(str(source))['normalize_name']
    suite = unittest.TestSuite()

    def valid(value, expected):
        if function(value) != expected:
            raise AssertionError('normalized value differs from the contract')

    def invalid(value, exception):
        try:
            function(value)
        except exception:
            return
        raise AssertionError('expected validation exception was not raised')

    for index, (value, expected) in enumerate([
        (' Ada\tLOVELACE ', 'ada lovelace'), ('STRAẞE', 'strasse'),
        (' One\nTwo ', 'one two'), ('A', 'a'),
    ]):
        suite.addTest(unittest.FunctionTestCase(
            lambda value=value, expected=expected: valid(value, expected),
            description='valid normalization case ' + str(index + 1)))
    for index, value in enumerate(['', ' \n\t ']):
        suite.addTest(unittest.FunctionTestCase(
            lambda value=value: invalid(value, ValueError),
            description='empty input case ' + str(index + 1)))
    for index, value in enumerate([None, 1, True, []]):
        suite.addTest(unittest.FunctionTestCase(
            lambda value=value: invalid(value, TypeError),
            description='invalid type case ' + str(index + 1)))
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    unchanged = hashlib.sha256(tests.read_bytes()).hexdigest() == manifest['protected_tests_sha256']
    if not unchanged:
        print('Protected tests changed during checks.', file=sys.stderr)
    return 0 if result.wasSuccessful() and result.testsRun == 10 and unchanged else 1


if __name__ == '__main__':
    sys.exit(main())
