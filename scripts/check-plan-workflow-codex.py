#!/usr/bin/env python3
"""Opt-in three-request native Codex planning/edit acceptance in a disposable project.

The test operator answers fixed questions and edits the proposal into one known
repair step. This tests reviewed execution, never autonomous plan quality/routing.
At most two edit-category one-shot approvals; all other requests are rejected.
"""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import sqlite3
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
SOURCE = 'def add_five(a, b, c, d, e):\n    return a + b + c + d + e + 1\n'
TESTS = '''import unittest
from calculator import add_five
class CalculatorTests(unittest.TestCase):
 def test_positive(self): self.assertEqual(add_five(1,2,3,4,5),15)
 def test_mixed(self): self.assertEqual(add_five(-1,-2,3,0,0),0)
 def test_zero(self): self.assertEqual(add_five(0,0,0,0,0),0)
'''
CASES = [(1,2,3,4,5), (-1,-2,3,0,0), (0,0,0,0,0),
         (0.25,0.5,0.75,1.0,1.5), (-5,-4,-3,-2,-1),
         (1000,-1000,3,4,5), (1,1,1,1,1)]
CLARIFIED = (
    'Corrige calculator.py: add_five(a,b,c,d,e) debe sumar exactamente esos cinco '
    'argumentos int/float usando aritmética Python, sin validación adicional ni '
    'redondeo. Hoy agrega incorrectamente 1. test_calculator.py contiene tres '
    'pruebas: (1,2,3,4,5)=15; (-1,-2,3,0,0)=0; (0,0,0,0,0)=0, todas fallan '
    'por ese +1. Mantén la firma y las pruebas intactas. Sólo edita calculator.py. '
    'No agregues archivos, interfaz ni despliegue. El operador ejecutará unittest '
    'y casos adicionales; el agente no debe ejecutar comandos ni afirmar que corrió pruebas.'
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_project(workspace):
    result = subprocess.run([sys.executable, '-B', '-m', 'unittest', '-v'],
        cwd=workspace, capture_output=True, timeout=10)
    return result.returncode == 0 and b'Ran 3 tests' in result.stderr


def check_holdouts(workspace):
    program = ('import runpy\nf=runpy.run_path("calculator.py")["add_five"]\n'
               + 'cases=' + repr(CASES) + '\n'
               + 'assert all(f(*case)==sum(case) for case in cases)\n')
    result = subprocess.run([sys.executable, '-I', '-B', '-c', program],
        cwd=workspace, capture_output=True, timeout=10)
    return result.returncode == 0


def complete(page, api, url, permit_edit=False):
    from playwright.sync_api import expect
    page.locator('#conversation-submit').click()
    approved = rejected = 0
    deadline = time.monotonic() + 100
    while time.monotonic() < deadline:
        cards = page.locator('.permission-card')
        if cards.count():
            card = cards.first
            kind = card.locator('h2').inner_text().removesuffix(' request')
            if permit_edit and kind == 'edit' and approved < 2:
                card.locator('button.allow').click()
                expect(card.locator('button.allow')).to_have_text('Confirm allow once')
                card.locator('button.allow').click()
                approved += 1
            else:
                card.locator('button.reject').click()
                rejected += 1
            page.wait_for_timeout(700)
        snapshot = api['snapshot'](page, url)
        if snapshot['turns'] and snapshot['turns'][-1]['status'] in ['completed','failed','cancelled']:
            break
        page.wait_for_timeout(200)
    else:
        raise ValueError('Workflow turn deadline exceeded; no retry')
    expect(page.locator('#conversation-state')).to_have_text('completed', timeout=2000)
    return snapshot, {'allowed_edits': approved, 'rejected_requests': rejected}


def propose(page, objective, api, url):
    from playwright.sync_api import expect
    page.locator('#project-plan').evaluate('el => el.open = true')
    page.locator('#plan-objective').fill(objective)
    page.locator('#plan-propose').click()
    snapshot, permissions = complete(page, api, url)
    page.locator('#plan-result-review').click()
    expect(page.locator('#plan-proposal-preview')).to_be_visible()
    assert snapshot['project_plan']['plan'] is None
    return snapshot, permissions


def run(args):
    from playwright.sync_api import sync_playwright, expect
    api = runpy.run_path(str(ROOT/'scripts/check-codex-browser.py'))
    private = runpy.run_path(str(ROOT/'scripts/check-planning-codex.py'))['write_private']
    args.output.mkdir(parents=True, mode=0o700, exist_ok=False)
    evidence = {'source_commit': subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        'dirty_candidate': bool(subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()),
        'codex_version': '0.159.2', 'requested_model': args.model, 'requested_effort': args.effort,
        'operator': 'fixed test reviewer', 'automatic_routing': False, 'tokens': None, 'cost': None}
    private(args.output/'evidence.json', evidence)
    with api['session'](args) as (url, workspace, database), sync_playwright() as driver:
        (workspace/'calculator.py').write_text(SOURCE)
        (workspace/'test_calculator.py').write_text(TESTS)
        original_test = digest(workspace/'test_calculator.py')
        assert not check_project(workspace) and not check_holdouts(workspace)
        browser = driver.chromium.launch(executable_path=str(args.chromium),args=['--no-sandbox'])
        try:
            page = browser.new_page(viewport={'width':1440,'height':1100})
            errors = []; page.on('pageerror',lambda _:errors.append(True))
            page.goto(url)
            expect(page.locator('#conversation-state')).to_have_text('ready')
            initial, first_permissions = propose(page, 'Corrige una prueba que falla en mi calculadora existente; todavía no he dado el error ni su contrato.', api, url)
            assert (workspace/'calculator.py').read_text() == SOURCE
            assert digest(workspace/'test_calculator.py') == original_test
            assert sorted(p.name for p in workspace.iterdir()) == ['calculator.py','test_calculator.py']
            private(args.output/'initial.json', {'response':initial['turns'][-1]['response'],'permissions':first_permissions})
            clarified, second_permissions = propose(page, CLARIFIED, api, url)
            assert (workspace/'calculator.py').read_text() == SOURCE
            assert digest(workspace/'test_calculator.py') == original_test
            assert sorted(p.name for p in workspace.iterdir()) == ['calculator.py','test_calculator.py']
            private(args.output/'clarified.json', {'response':clarified['turns'][-1]['response'],'permissions':second_permissions})
            # The reviewer replaces variable generated stages with this fixed scoped
            # repair, rather than allowing generated text to drive approvals/actions.
            page.locator('#plan-proposal-apply').click()
            page.locator('#plan-objective').fill(CLARIFIED)
            while page.locator('#plan-builder fieldset').count() > 1:
                page.locator('#plan-builder fieldset').last.get_by_role('button',name='Remove step').click()
            page.locator('#plan-builder .plan-title').fill('Repair the extra +1 only')
            page.locator('#plan-builder .plan-scope').fill('Read calculator.py and test_calculator.py. Edit only calculator.py to remove the extra +1, preserving add_five(a,b,c,d,e). Do not modify the test file, create files, run commands, delegate or change anything else. Use the file edit tool. If denied, stop.')
            page.locator('#plan-builder .plan-acceptance').fill('Report the exact change, without claiming tests ran. The operator independently runs the unchanged three tests and seven arithmetic cases before reviewing this stage.')
            page.locator('#plan-review').click()
            expect(page.locator('#plan-board .plan-step')).to_have_count(1)
            assert len(api['snapshot'](page,url)['turns']) == 2
            page.reload()
            expect(page.locator('#plan-board .plan-step')).to_have_count(1)
            page.locator('#project-plan').evaluate('el => el.open = true')
            page.locator('#plan-board').get_by_role('button',name='Prepare step draft',exact=True).click()
            result, permissions = complete(page,api,url,permit_edit=True)
            assert permissions['allowed_edits'] > 0
            assert sorted(p.name for p in workspace.iterdir()) == ['calculator.py','test_calculator.py']
            assert digest(workspace/'test_calculator.py') == original_test
            # Only the known one-line repair may run in the host check process.
            # Reject extra code before importing the agent-edited module.
            assert (workspace/'calculator.py').read_text() == SOURCE.replace(' + 1\n', '\n')
            assert check_project(workspace) and check_holdouts(workspace)
            page.locator('#plan-board').get_by_role('button',name='I reviewed this result').click()
            expect(page.locator('#plan-progress')).to_contain_text('1/1 operator-reviewed')
            final = api['snapshot'](page,url)
            assert len(final['turns']) == 3
            assert final['project_plan']['plan']['steps'][0]['reviewed']
            last = final['turns'][-1]['turn_id']
            evidence.update({'turn_count':3,'all_turns_completed':all(t['status']=='completed' for t in final['turns']),
                'clarification_permissions': [first_permissions,second_permissions], 'repair_permissions':permissions,
                'unchanged_tests':True,'host_unittest_count':3,'independent_arithmetic_cases':len(CASES),
                'plan_survived_refresh':True,'reviewed_steps':1,'turn_metrics':final['turn_metrics'][last],
                'reported_controls':final.get('context',{}).get('startup_controls')})
            with sqlite3.connect(database) as connection:
                receipts = [json.loads(row[0]) for row in connection.execute('select data from events where kind=?', ('"Permission"',))]
            assert receipts and all(r['authorization']=='one_shot' for r in receipts)
            evidence['redacted_one_shot_receipts'] = len(receipts)
            assert not errors
            page.screenshot(path=str(args.output/'workflow-completed.png'),full_page=True)
            page.set_viewport_size({'width':390,'height':844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth + 1')
            page.screenshot(path=str(args.output/'workflow-completed-mobile.png'),full_page=True)
            for image in args.output.glob('*.png'):
                image.chmod(0o600)
            private(args.output/'repair.json', {'response':result['turns'][-1]['response'],'source':(workspace/'calculator.py').read_text()})
            private(args.output/'evidence.json', evidence)
        finally:
            browser.close()
    print('PASS: three native turns, reviewed clarified repair, one-shot edit, unchanged tests, 3 unit tests and 7 independent arithmetic cases, desktop/mobile')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--live-account',action='store_true')
    for name in ['urdia','codex','chromium','output']:
        parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--model',required=True)
    parser.add_argument('--effort',choices=['none','minimal','low','medium','high','xhigh'],default='low')
    args = parser.parse_args()
    if not args.live_account:
        parser.error('--live-account is required; three requests consume account quota')
    args.output = args.output.resolve()
    if args.output.is_relative_to(ROOT):
        parser.error('Private evidence must stay outside the checkout')
    for name in ['urdia','codex','chromium']:
        setattr(args,name,getattr(args,name).resolve(strict=True))
    version = subprocess.run([str(args.codex),'--version'],capture_output=True,text=True,check=True,timeout=10).stdout.strip()
    if version != 'codex-cli 0.159.2':
        raise ValueError('Unsupported pinned Codex runtime')
    run(args)


if __name__ == '__main__':
    try:
        main()
    except Exception:
        raise SystemExit('Workflow check stopped; private diagnostics omitted; inspect private evidence')
