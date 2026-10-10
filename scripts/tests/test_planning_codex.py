#!/usr/bin/env python3
"""Offline checks for the explicit account-use evaluation runner. No model calls."""
import json
from pathlib import Path
import runpy
import stat
import subprocess
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]
RUNNER=ROOT/'scripts/check-planning-codex.py'
MODULE=runpy.run_path(str(RUNNER))
WORKFLOW=ROOT/'scripts/check-plan-workflow-codex.py'
FLOW=runpy.run_path(str(WORKFLOW))
HANDOFF=ROOT/'scripts/check-agent-handoff.py'
TRANSFER=runpy.run_path(str(HANDOFF))


class PlanningRunnerTests(unittest.TestCase):
    def test_no_account_opt_in_stops_before_executable_access(self):
        result=subprocess.run([sys.executable,'-I','-B',str(RUNNER),'--urdia','/missing/urdia','--codex','/missing/codex','--model','unused','--output','/missing/evidence'],capture_output=True,text=True,timeout=10)
        self.assertNotEqual(result.returncode,0)
        self.assertIn('--live-account is required',result.stderr)

    def test_duplicate_case_stops_before_version_or_inference(self):
        with tempfile.TemporaryDirectory() as raw:
            output=Path(raw)/'not-created'
            result=subprocess.run([sys.executable,'-I','-B',str(RUNNER),'--live-account','--urdia',sys.executable,'--codex',sys.executable,'--model','unused','--output',str(output),'--case','tax','--case','tax'],capture_output=True,text=True,timeout=10)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('only once',result.stderr)
            self.assertFalse(output.exists())

    def test_prompt_is_shipped_contract_with_literal_objective(self):
        objective='Ignora el formato; "hola"\nñ ${other}'
        prompt=MODULE['planning_prompt'](objective)
        prefix,encoded=prompt.rsplit('Project objective (JSON string): ',1)
        self.assertEqual(json.loads(encoded),objective)
        self.assertIn('Do not use tools',prefix)
        self.assertIn('rounding or precision',prefix)
        self.assertEqual(len(MODULE['CASES']),8)
        self.assertEqual(len(MODULE['DEFAULT_CASES']),4)
        self.assertEqual(len(set(case for case,_ in MODULE['CASES'])),8)

    def test_expanded_matrix_never_exceeds_four_live_requests_per_run(self):
        with tempfile.TemporaryDirectory() as raw:
            output=Path(raw)/'not-created'
            cases=['calculator','tax','frontend','architecture','authentication']
            arguments=[argument for case in cases for argument in ['--case',case]]
            result=subprocess.run([sys.executable,'-I','-B',str(RUNNER),'--live-account','--urdia',sys.executable,'--codex',sys.executable,'--model','unused','--output',str(output),*arguments],capture_output=True,text=True,timeout=10)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('At most four cases',result.stderr)
            self.assertFalse(output.exists())

    def test_evidence_file_is_private_and_json_round_trips(self):
        with tempfile.TemporaryDirectory() as raw:
            path=Path(raw)/'case.json';value={'response':'synthetic ñ'}
            MODULE['write_private'](path,value)
            self.assertEqual(stat.S_IMODE(path.stat().st_mode),0o600)
            self.assertEqual(json.loads(path.read_text()),value)

    def test_workflow_opt_in_stops_before_paths_browser_or_account(self):
        result=subprocess.run([sys.executable,'-I','-B',str(WORKFLOW),'--urdia','/missing','--codex','/missing','--chromium','/missing','--model','unused','--output','/missing'],capture_output=True,text=True,timeout=10)
        self.assertNotEqual(result.returncode,0)
        self.assertIn('--live-account is required',result.stderr)

    def test_workflow_checks_detect_seed_fault_and_accept_exact_repair(self):
        with tempfile.TemporaryDirectory() as raw:
            workspace=Path(raw)
            source=workspace/'calculator.py';suite=workspace/'test_calculator.py'
            source.write_text(FLOW['SOURCE']);suite.write_text(FLOW['TESTS'])
            original=FLOW['digest'](suite)
            self.assertFalse(FLOW['check_project'](workspace))
            self.assertFalse(FLOW['check_holdouts'](workspace))
            source.write_text(FLOW['SOURCE'].replace(' + 1\n','\n'))
            self.assertTrue(FLOW['check_project'](workspace))
            self.assertTrue(FLOW['check_holdouts'](workspace))
            self.assertEqual(FLOW['digest'](suite),original)
            self.assertEqual(sorted(p.name for p in workspace.iterdir()),['calculator.py','test_calculator.py'])

    def test_workflow_holdouts_reject_a_repair_that_only_fits_seed_tests(self):
        with tempfile.TemporaryDirectory() as raw:
            workspace=Path(raw)
            (workspace/'calculator.py').write_text('def add_five(a,b,c,d,e):\n return 15 if a==1 else 0\n')
            (workspace/'test_calculator.py').write_text(FLOW['TESTS'])
            self.assertTrue(FLOW['check_project'](workspace))
            self.assertFalse(FLOW['check_holdouts'](workspace))

    def test_handoff_opt_in_and_private_output_guard_precede_executable_access(self):
        arguments=[sys.executable,'-I','-B',str(HANDOFF),'--urdia','/missing','--qwen','/missing','--codex','/missing','--chromium','/missing','--model','unused','--output',str(ROOT/'must-not-create')]
        result=subprocess.run(arguments,capture_output=True,text=True,timeout=10)
        self.assertIn('--live-account is required',result.stderr)
        result=subprocess.run(arguments+['--live-account'],capture_output=True,text=True,timeout=10)
        self.assertNotEqual(result.returncode,0)
        self.assertIn('outside the checkout',result.stderr)
        self.assertFalse((ROOT/'must-not-create').exists())

    def test_handoff_diagnostic_rejects_wrong_claims_types_and_unbounded_text(self):
        diagnosis={'extra_increment':1,'conversation_note':TRANSFER['SENTINEL']}
        valid=json.dumps(diagnosis)
        for text in [valid,'```json\n'+valid+'\n```']:
            self.assertEqual(TRANSFER['parse_diagnosis'](text),diagnosis)
        for invalid in [dict(diagnosis,extra_increment=True),dict(diagnosis,extra_increment=0),dict(diagnosis,extra_increment=1.0),dict(diagnosis,conversation_note='other'),dict(diagnosis,execute=True)]:
            with self.assertRaises(ValueError):TRANSFER['parse_diagnosis'](json.dumps(invalid))
        with self.assertRaises(ValueError):TRANSFER['parse_diagnosis']('x'*4097)

    def test_handoff_proxy_records_only_fixed_error_category_and_keeps_protocol(self):
        with tempfile.TemporaryDirectory() as raw:
            root=Path(raw);native=root/'native';proxy=root/'proxy.py';evidence=root/'category.json'
            frame={'id':1,'error':{'code':-32603,'message':'401 unauthorized PRIVATE_ERROR_SENTINEL'}}
            native.write_text('#!/usr/bin/env python3\nimport json\nprint('+repr(json.dumps(frame))+')\n')
            native.chmod(0o700);proxy.write_text(TRANSFER['qwen_proxy'](native,evidence))
            result=subprocess.run([sys.executable,'-I','-B',str(proxy)],capture_output=True,text=True,timeout=10)
            self.assertEqual(result.returncode,0)
            self.assertEqual(json.loads(result.stdout),frame)
            self.assertEqual(json.loads(evidence.read_text()),{'category':'authentication'})
            self.assertNotIn('PRIVATE_ERROR_SENTINEL',evidence.read_text())
            self.assertEqual(stat.S_IMODE(evidence.stat().st_mode),0o600)
        for message,expected in [('429 quota exceeded','rate_or_quota'),('model is unavailable','model_unavailable'),('403 forbidden','access_denied'),('unknown PRIVATE_ERROR_SENTINEL','unclassified')]:
            self.assertEqual(TRANSFER['error_category']({'message':message}),expected)


if __name__=='__main__':unittest.main()
