#!/usr/bin/env python3
"""Opt-in two-request reviewed agent/profile handoff; never ordinary CI.

Qwen or Codex diagnoses supplied synthetic code. A fresh pinned Codex profile repairs
the disposable file after explicit review. No transcript forwarding or auto routing.
"""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import runpy
import selectors
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
OBJECTIVE = ('Repair add_five(a,b,c,d,e) in calculator.py: sum exactly five int/float '
    'arguments using Python arithmetic, no extra increment, validation or rounding. '
    'Preserve the signature and test_calculator.py. The existing implementation '
    'returns a+b+c+d+e+1. Only calculator.py may change. The operator runs tests '
    'and seven independent arithmetic cases; agents must not run commands or '
    'claim that tests ran. No new files, UI or deployment.')
SENTINEL = 'TRANSCRIPT_ONLY_DIAGNOSTIC_NOTE'


def error_category(error):
    # Only fixed categories leave process memory; never save native messages/data.
    text = json.dumps(error).lower()
    if re.search(r'\b401\b|unauthorized|authentication required|not authenticated|invalid api key', text):
        return 'authentication'
    if re.search(r'\b429\b|quota|rate.limit|usage.limit', text):
        return 'rate_or_quota'
    if re.search(r'model.{0,30}(not found|unavailable|unsupported|does not exist)', text):
        return 'model_unavailable'
    if re.search(r'\b403\b|forbidden|access.denied', text):
        return 'access_denied'
    return 'unclassified'


def qwen_proxy(real, evidence_path):
    # The adapter discards private provider errors. This opt-in test-only launcher
    # records their category while forwarding unchanged ACP frames, not raw logs.
    return '''#!/usr/bin/env python3
import json,os,runpy,subprocess,sys
category=runpy.run_path(''' + repr(str(Path(__file__).resolve())) + ''')['error_category']
p=subprocess.Popen([''' + repr(str(real)) + ''',*sys.argv[1:]],stdin=sys.stdin,stdout=subprocess.PIPE,stderr=sys.stderr)
try:
 while True:
  line=p.stdout.readline(1048577)
  if not line: break
  if len(line)>1048576: raise ValueError('Native frame limit')
  try:
   frame=json.loads(line)
   if isinstance(frame,dict) and isinstance(frame.get('error'),dict):
    with open(''' + repr(str(evidence_path)) + ''','w') as output:
     os.fchmod(output.fileno(),0o600)
     json.dump({'category':category(frame['error'])},output)
  except (ValueError,TypeError): pass
  sys.stdout.buffer.write(line);sys.stdout.buffer.flush()
 sys.exit(p.wait())
finally:
 if p.poll() is None:p.terminate();p.wait(timeout=5)
'''


def parse_diagnosis(text):
    text = text.strip()
    if len(text.encode()) > 4096:
        raise ValueError('Diagnostic exceeds fixture limit')
    if text.startswith('```json\n') and text.endswith('```'):
        text = text[8:-3].strip()
    diagnosis = json.loads(text)
    if (not isinstance(diagnosis, dict)
            or set(diagnosis) != {'extra_increment', 'conversation_note'}
            or type(diagnosis['extra_increment']) is not int
            or diagnosis['extra_increment'] != 1
            or diagnosis['conversation_note'] != SENTINEL):
        raise ValueError('Diagnostic differs from fixed fixture contract')
    return diagnosis


@contextmanager
def session(args):
    probe = runpy.run_path(str(ROOT/'scripts/check-codex-browser.py'))
    with tempfile.TemporaryDirectory(prefix='urdia-agent-handoff-') as raw:
        root = Path(raw); workspace = root/'Calculator'; workspace.mkdir()
        if args.diagnostic_agent == 'qwen':
            proxy = root/'qwen-proxy'
            proxy.write_text(qwen_proxy(args.qwen,args.output/'qwen-error-category.json'))
            proxy.chmod(0o700)
            command = str(proxy); arguments = ['--acp']; name = 'Qwen diagnostic'
        else:
            command = sys.executable
            arguments = ['-I','-B',str(ROOT/'scripts/codex-acp.py'),'--codex',str(args.codex),
                         '--model',args.model,'--effort',args.diagnostic_effort]
            name = 'Codex diagnostic'
        config = root/'providers.toml'
        config.write_text('[[provider]]\nid="handoff-source"\nname='+json.dumps(name)+'\ncommand='
            + json.dumps(command) + '\nargs='+json.dumps(arguments)+'\n'
            + '[[provider]]\nid="handoff-codex"\nname="Codex handoff"\ncommand='
            + json.dumps(sys.executable) + '\nargs='
            + json.dumps(['-I','-B',str(ROOT/'scripts/codex-acp.py'),'--codex',str(args.codex),
                          '--model',args.model,'--effort',args.effort])
            + '\n[[profile]]\nid="codex-repair"\nname="Codex repair profile"\n'
            + 'provider="handoff-codex"\nagent="Codex native"\ninference_provider="Existing Codex account"\nmodel='
            + json.dumps(args.model) + '\neffort=' + json.dumps(args.effort)
            + '\nsession_controls={model=' + json.dumps(args.model) + ',effort=' + json.dumps(args.effort) + '}\n')
        database = root/'events.sqlite'
        with tempfile.TemporaryFile() as errors:
            process = subprocess.Popen([str(args.urdia),'--config',str(config),'chat','--provider','handoff-source',
                '--workspace',str(workspace),'--database',str(database),'--permissions-ui','--bind','127.0.0.1:0'],
                stdout=subprocess.PIPE,stderr=errors,start_new_session=True)
            try:
                raw_output = b''; url = None; deadline = time.monotonic()+30
                with selectors.DefaultSelector() as selector:
                    selector.register(process.stdout,selectors.EVENT_READ)
                    while time.monotonic()<deadline and len(raw_output)<65536:
                        if selector.select(.1): raw_output += os.read(process.stdout.fileno(),4096)
                        try: url = probe['private_url'](raw_output); break
                        except ValueError: pass
                        if process.poll() is not None: break
                if url is None: raise ValueError('Handoff startup failed; diagnostics omitted')
                yield url,workspace,database
            finally:
                if process.poll() is None:
                    os.killpg(process.pid,signal.SIGINT)
                    try: process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
                process.stdout.close()


def run(args):
    from playwright.sync_api import sync_playwright, expect
    probe = runpy.run_path(str(ROOT/'scripts/check-codex-browser.py'))
    flow = runpy.run_path(str(ROOT/'scripts/check-plan-workflow-codex.py'))
    private = runpy.run_path(str(ROOT/'scripts/check-planning-codex.py'))['write_private']
    args.output.mkdir(parents=True,mode=0o700,exist_ok=False)
    evidence = {'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        'dirty_candidate':bool(subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()),
        'requested_codex_model':args.model,'requested_codex_effort':args.effort,
        'diagnostic_agent':args.diagnostic_agent,
        'diagnostic_runtime_version':'0.24.7' if args.diagnostic_agent=='qwen' else '0.159.2',
        'requested_diagnostic_effort':args.diagnostic_effort if args.diagnostic_agent=='codex' else None,
        'codex_version':'0.159.2','tokens':None,'cost':None,
        'automatic_routing':False,'stage':'starting'}
    private(args.output/'evidence.json',evidence)
    with session(args) as (url,workspace,_), sync_playwright() as driver:
        (workspace/'calculator.py').write_text(flow['SOURCE'])
        (workspace/'test_calculator.py').write_text(flow['TESTS'])
        original_test = flow['digest'](workspace/'test_calculator.py')
        assert not flow['check_project'](workspace) and not flow['check_holdouts'](workspace)
        browser = driver.chromium.launch(executable_path=str(args.chromium),args=['--no-sandbox'])
        try:
            page = browser.new_page(viewport={'width':1440,'height':1100})
            errors=[];page.on('pageerror',lambda _:errors.append(True));page.goto(url)
            expect(page.locator('#conversation-state')).to_have_text('ready',timeout=15000)
            page.locator('#project-plan').evaluate('el => el.open = true')
            page.locator('#plan-objective').fill(OBJECTIVE)
            page.locator('#plan-manual').evaluate('el => el.open = true')
            for index in range(2):
                if index:page.locator('#plan-add').click()
                row=page.locator('#plan-builder fieldset').nth(index)
                row.locator('.plan-title').fill(['Diagnose supplied source','Repair calculator.py'][index])
                row.locator('.plan-scope').fill([
                    'Use only this supplied source, without tools, reading files, edits or commands: '+flow['SOURCE']
                    +' Return exactly JSON {"extra_increment":1,"conversation_note":"'+SENTINEL+'"}. The note belongs only to this diagnostic conversation.',
                    'Read only calculator.py and test_calculator.py; edit only calculator.py to remove the extra +1. '
                    'Preserve add_five(a,b,c,d,e). Do not modify tests, add files, delegate, run commands or start other steps. '
                    'Use the file edit tool; if denied, stop.'
                ][index])
                row.locator('.plan-acceptance').fill([
                    'Identify the incorrect increment; keep both files unchanged.',
                    'Report the change honestly. The operator independently runs the unchanged three tests and seven arithmetic cases before reviewing.'
                ][index])
            page.locator('#plan-review').click()
            first=page.locator('#plan-board .plan-step').nth(0);second=page.locator('#plan-board .plan-step').nth(1)
            expect(second.get_by_role('button',name='Prepare step draft',exact=True)).to_be_disabled()
            first.get_by_role('button',name='Prepare step draft',exact=True).click()
            evidence['stage']='diagnostic';private(args.output/'evidence.json',evidence)
            try:
                diagnostic, permissions = flow['complete'](page,probe,url)
            except Exception:
                observed=probe['snapshot'](page,url)
                evidence.update({'stage':'diagnostic-stopped',
                    'turn_count':len(observed['turns']),
                    'observed_status':observed['turns'][-1]['status'] if observed['turns'] else None,
                    'diagnostic_reported_controls':observed.get('context',{}).get('startup_controls')})
                private(args.output/'evidence.json',evidence)
                raise
            text = diagnostic['turns'][-1]['response'].strip()
            parse_diagnosis(text)
            assert (workspace/'calculator.py').read_text()==flow['SOURCE']
            assert flow['digest'](workspace/'test_calculator.py')==original_test
            private(args.output/'diagnostic.json',{'response':text,'permissions':permissions})
            first.get_by_role('button',name='I reviewed this result').click()
            expect(second.get_by_role('button',name='Prepare step draft',exact=True)).to_be_enabled()
            page.locator('#next-profile-select').select_option('codex-repair')
            page.locator('#next-profile-apply').click()
            expect(page.locator('#next-profile-active')).to_contain_text('Codex repair profile')
            assert len(probe['snapshot'](page,url)['turns'])==1
            second.get_by_role('button',name='Prepare step draft',exact=True).click()
            prompt = page.locator('#conversation-input').input_value()
            assert OBJECTIVE in prompt and SENTINEL not in prompt
            evidence['stage']='codex-repair';private(args.output/'evidence.json',evidence)
            result, repair_permissions = flow['complete'](page,probe,url,permit_edit=True)
            assert repair_permissions['allowed_edits'] > 0
            assert sorted(p.name for p in workspace.iterdir())==['calculator.py','test_calculator.py']
            assert flow['digest'](workspace/'test_calculator.py')==original_test
            source=(workspace/'calculator.py').read_text()
            assert source==flow['SOURCE'].replace(' + 1\n','\n')
            assert flow['check_project'](workspace) and flow['check_holdouts'](workspace)
            turn=result['turns'][-1];execution=turn['execution']
            assert turn['prompt']==prompt and execution['provider_id']=='handoff-codex'
            assert execution['startup_controls']=={'model':args.model,'effort':args.effort}
            assert execution['profile']['selection']=='manual-next-task'
            assert result['project_plan']['plan']['steps'][1]['task']['turn_id']==turn['turn_id']
            second.get_by_role('button',name='I reviewed this result').click()
            expect(page.locator('#plan-progress')).to_contain_text('2/2 operator-reviewed')
            final=probe['snapshot'](page,url);assert len(final['turns'])==2
            base,token=url.split('#token=',1)
            timeline=page.request.get(base+'api/timeline',headers={'Authorization':'Bearer '+token}).text()
            assert timeline.count('<details class="session"')==2
            evidence.update({'stage':'passed','turn_count':2,'original_provider':final['context']['provider_id'],
                'diagnostic_reported_controls':final['context'].get('startup_controls'),
                'codex_reported_controls':execution['startup_controls'],
                'diagnostic_permissions':permissions,'repair_permissions':repair_permissions,
                'host_unittest_count':3,'independent_arithmetic_cases':7,
                'test_file_unchanged':True,'earlier_note_absent_from_handoff_prompt':True,
                'distinct_sessions':2,'linked_stage_reviewed':True,'turn_metrics':final['turn_metrics'][turn['turn_id']]})
            private(args.output/'repair.json',{'response':turn['response'],'source':source})
            private(args.output/'evidence.json',evidence)
            assert not errors
            page.screenshot(path=str(args.output/'handoff-completed.png'),full_page=True)
            page.set_viewport_size({'width':390,'height':844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth + 1')
            page.screenshot(path=str(args.output/'handoff-completed-mobile.png'),full_page=True)
            for image in args.output.glob('*.png'):image.chmod(0o600)
        finally:browser.close()
    print('PASS: '+args.diagnostic_agent+' diagnosis to fresh Codex reviewed repair, explicit requirements without transcript note, 3 unchanged tests and 7 independent cases')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--live-account',action='store_true')
    for name in ['urdia','codex','chromium','output']:parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--qwen',type=Path)
    parser.add_argument('--diagnostic-agent',choices=['qwen','codex'],default='qwen')
    parser.add_argument('--diagnostic-effort',choices=['none','minimal','low','medium','high','xhigh'],default='low')
    parser.add_argument('--model',required=True)
    parser.add_argument('--effort',choices=['none','minimal','low','medium','high','xhigh'],default='low')
    args=parser.parse_args()
    if not args.live_account:parser.error('--live-account is required; two model requests consume account quota')
    if args.diagnostic_agent=='qwen' and args.qwen is None:parser.error('--qwen is required for Qwen diagnosis')
    args.output=args.output.resolve()
    if args.output.is_relative_to(ROOT):parser.error('Private evidence must stay outside the checkout')
    names=['urdia','codex','chromium'] + (['qwen'] if args.diagnostic_agent=='qwen' else [])
    for name in names:setattr(args,name,getattr(args,name).resolve(strict=True))
    versions=[(args.codex,'codex-cli 0.159.2')] + ([(args.qwen,'0.24.7')] if args.diagnostic_agent=='qwen' else [])
    for executable,expected in versions:
        version=subprocess.run([str(executable),'--version'],capture_output=True,text=True,check=True,timeout=10).stdout.strip()
        if version!=expected:raise ValueError('Unsupported pinned agent runtime')
    run(args)


if __name__=='__main__':
    try:main()
    except Exception:raise SystemExit('Agent handoff check stopped; private diagnostics omitted; inspect private evidence')
