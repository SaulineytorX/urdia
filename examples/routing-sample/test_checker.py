"""Offline calibration: rejected seeds, accepted references, protected tests."""
import importlib.util
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('sample_check', Path(__file__).with_name('check.py'))
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)
REFERENCES = {
    'intervals': '''def merge_intervals(intervals):
 if not isinstance(intervals,(list,tuple)): raise TypeError()
 for p in intervals:
  if not isinstance(p,(list,tuple)) or len(p)!=2: raise TypeError()
  if any(type(v) is not int for v in p): raise TypeError()
  if p[0]>p[1]: raise ValueError()
 result=[]
 for a,b in sorted((p[0],p[1]) for p in intervals):
  if result and a<=result[-1][1]: result[-1]=(result[-1][0],max(result[-1][1],b))
  else: result.append((a,b))
 return result
''',
    'dependencies': '''def dependency_order(graph):
 if not isinstance(graph,dict): raise TypeError()
 for key,deps in graph.items():
  if not isinstance(key,str) or not isinstance(deps,(list,tuple)): raise TypeError()
  if not key: raise ValueError()
  for dep in deps:
   if not isinstance(dep,str): raise TypeError()
   if not dep or dep not in graph: raise ValueError()
 pending={k:set(v) for k,v in graph.items()}
 result=[]
 while pending:
  ready=sorted(k for k,v in pending.items() if not v)
  if not ready: raise ValueError()
  chosen=ready[0];result.append(chosen);pending.pop(chosen)
  for deps in pending.values(): deps.discard(chosen)
 return result
''',
}


class CheckerTests(unittest.TestCase):
    def test_frozen_bindings_and_balanced_order(self):
        base = Path(__file__).parent
        protocol = json.loads((base / 'protocol.json').read_text())
        self.assertEqual(hashlib.sha256((base / 'cases.json').read_bytes()).hexdigest(),
                         protocol['fixture_sha256'])
        self.assertEqual(hashlib.sha256((base / 'check.py').read_bytes()).hexdigest(),
                         protocol['checker_sha256'])
        expected = [('manual', 1), ('jev', 1), ('jev', 2), ('manual', 2)]
        for case in checker.CASES:
            rows = [r for r in protocol['order'] if r['case'] == case['id']]
            self.assertEqual([(r['condition'], r['repeat']) for r in rows], expected)

    def test_all_observed_outputs_replay_without_models(self):
        base = Path(__file__).parent
        report = json.loads((base / 'observed.json').read_text())
        self.assertEqual(hashlib.sha256((base / 'protocol.json').read_bytes()).hexdigest(),
                         report['protocol_sha256'])
        self.assertEqual(len(report['runs']), 8)
        for row in report['runs']:
            case = next(c for c in checker.CASES if c['id'] == row['case'])
            source = base / row['source_artifact']
            self.assertEqual(checker.digest(source), row['source_sha256'])
            with self.subTest(case=row['case'], mode=row['condition'], repeat=row['repeat']), tempfile.TemporaryDirectory() as tmp:
                w = Path(tmp)
                (w / 'test_contract.py').write_text(case['tests'])
                (w / 'solution.py').write_bytes(source.read_bytes())
                self.assertEqual(checker.check(case, w), row['deterministic_checks'])

    def test_seeds_fail_and_references_pass(self):
        for case in checker.CASES:
            with self.subTest(case=case['id']), tempfile.TemporaryDirectory() as tmp:
                w = Path(tmp)
                (w / 'test_contract.py').write_text(case['tests'])
                (w / 'solution.py').write_text(case['seed'])
                self.assertFalse(checker.check(case, w)['passed'])
                (w / 'solution.py').write_text(REFERENCES[case['id']])
                self.assertTrue(checker.check(case, w)['passed'])
                (w / 'test_contract.py').write_text(case['tests'] + '\n')
                with self.assertRaisesRegex(ValueError, 'protected_tests_changed'):
                    checker.check(case, w)

    def test_source_screen_rejects_execution_and_dynamic_access(self):
        for source in ['import os', 'def f():\n return open("x")',
                       'def f():\n return (1).__class__',
                       'def f(x=print("side effect")):\n return x']:
            with self.subTest(source=source), self.assertRaises(ValueError):
                checker.screen(source)

    def test_expected_files_only_and_no_symlinks(self):
        case = checker.CASES[0]
        with tempfile.TemporaryDirectory() as tmp:
            w = Path(tmp)
            (w / 'test_contract.py').write_text(case['tests'])
            (w / 'solution.py').symlink_to(w / 'test_contract.py')
            with self.assertRaisesRegex(ValueError, 'regular_files_required'):
                checker.check(case, w)
            (w / 'other.py').write_text('')
            with self.assertRaisesRegex(ValueError, 'workspace_files'):
                checker.check(case, w)


if __name__ == '__main__':
    unittest.main()
