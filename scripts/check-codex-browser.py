#!/usr/bin/env python3
"""Explicit opt-in real-account Codex dashboard acceptance; never run in normal CI.

Two small model tasks in an isolated disposable workspace. Reject the first edit;
allow at most two expected edit requests in the second. No execution approvals,
trusted access, credential copying, browser traces or raw transcript exports.
Requires separately installed Playwright and local Chrome.
"""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import selectors
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
PROMPT = (
    'Create only calculator.py with add_five(a,b,c,d,e) returning the sum of those five numbers, '
    'and test_calculator.py using unittest with exactly three tests: '
    '(1,2,3,4,5) gives 15; (-1,-2,3,0,0) gives 0; (0,0,0,0,0) gives 0. '
    'Use the file edit tool only. Do not run commands, read unrelated files, delegate or change '
    'anything else. If an edit is denied, stop without retrying. Report accurately; do not claim '
    'tests ran. The host will run them independently.'
)


def private_url(raw):
    match = re.search(rb'http://127\.0\.0\.1:(\d+)/#token=([0-9a-f]{64})\b', raw)
    if not match or not 0 < int(match[1]) <= 65535:
        raise ValueError('No valid private loopback URL')
    return match[0].decode()


def decision(kind, allow, approved):
    return 'allow_once' if allow and kind == 'edit' and 0 <= approved < 2 else 'reject_once'


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument('--live-account', action='store_true', help='Authorize two real account requests and scoped temporary edit approvals')
    result.add_argument('--urdia', type=Path, required=True)
    result.add_argument('--codex', type=Path, required=True)
    result.add_argument('--model', required=True)
    result.add_argument('--effort', choices=['none', 'minimal', 'low', 'medium', 'high', 'xhigh'], default='low')
    result.add_argument('--chromium', type=Path, required=True)
    result.add_argument('--output', type=Path, required=True)
    return result


@contextmanager
def session(args):
    with tempfile.TemporaryDirectory(prefix='urdia-codex-browser-') as tmp:
        root = Path(tmp)
        workspace = root/'Calculator preview'; workspace.mkdir()
        config = root/'providers.toml'
        config.write_text('[[provider]]\nid = "codex"\nname = "Codex native"\ncommand = '
            + json.dumps(sys.executable) + '\nargs = '
            + json.dumps(['-I', '-B', str(ROOT/'scripts/codex-acp.py'), '--codex', str(args.codex), '--model', args.model, '--effort', args.effort]) + '\n')
        database = root/'events.sqlite'
        with tempfile.TemporaryFile() as errors:
            process = subprocess.Popen([str(args.urdia), '--config', str(config), 'chat', '--provider', 'codex',
                '--workspace', str(workspace), '--database', str(database), '--permissions-ui', '--bind', '127.0.0.1:0'],
                stdout=subprocess.PIPE, stderr=errors, start_new_session=True)
            try:
                raw = b''; deadline = time.monotonic()+20; url = None
                with selectors.DefaultSelector() as selector:
                    selector.register(process.stdout, selectors.EVENT_READ)
                    while time.monotonic()<deadline and len(raw)<65536:
                        if selector.select(.1): raw += os.read(process.stdout.fileno(),4096)
                        try: url = private_url(raw); break
                        except ValueError: pass
                        if process.poll() is not None: break
                if url is None: raise ValueError('Codex dashboard startup failed; private diagnostics omitted')
                yield url, workspace, database
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    try: process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL); process.wait(timeout=5)
                process.stdout.close()


def snapshot(page, url):
    base, token = url.split('#token=',1)
    response = page.request.get(base+'api/conversation', headers={'Authorization':'Bearer '+token})
    if not response.ok: raise ValueError('Authenticated snapshot unavailable')
    return response.json()


def turn(page, allow, output):
    from playwright.sync_api import expect
    page.locator('#conversation-input').fill(PROMPT)
    page.locator('#conversation-submit').click()
    approved = rejected = 0
    deadline = time.monotonic()+100
    seen_permission = False
    while time.monotonic()<deadline:
        cards = page.locator('.permission-card')
        if cards.count():
            card = cards.first
            kind = card.locator('h2').inner_text().removesuffix(' request')
            expect(page.locator('#task-title')).to_have_text('Your decision is needed')
            if not seen_permission:
                page.screenshot(path=str(output/('codex-permission-allow.png' if allow else 'codex-permission-reject.png')), full_page=True)
                seen_permission = True
            if decision(kind, allow, approved)=='allow_once':
                card.locator('button.allow').click()
                expect(card.locator('button.allow')).to_have_text('Confirm allow once')
                card.locator('button.allow').click()
                approved += 1
            else:
                card.locator('button.reject').click()
                rejected += 1
            # Poll-driven cards may disappear before the next render. Avoid duplicate clicks.
            expect(page.locator('#status')).not_to_have_text('Could not send permission decision')
            page.wait_for_timeout(700)
        if page.locator('#task-title').inner_text() in ['Task completed','Task failed','Task stopped']:
            break
        page.wait_for_timeout(200)
    expect(page.locator('#task-title')).to_have_text('Task completed',timeout=1000)
    assert seen_permission, 'The model did not request an expected edit'
    assert (approved > 0 and rejected == 0) if allow else (approved == 0 and rejected > 0)
    expect(page.locator('#tool-activity')).to_be_visible()
    expect(page.locator('#tool-calls')).to_contain_text('Edit')
    expect(page.locator('#task-metrics')).to_be_visible()
    return {'allowed_edits':approved,'rejected_requests':rejected}


def run(args):
    from playwright.sync_api import sync_playwright, expect
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    with session(args) as (url, workspace, database), sync_playwright() as driver:
        browser = driver.chromium.launch(executable_path=str(args.chromium),args=['--no-sandbox'])
        try:
            page = browser.new_page(viewport={'width':1440,'height':1000})
            errors=[]; page.on('pageerror',lambda _: errors.append('browser error'))
            page.goto(url)
            expect(page.locator('#agent-name')).to_have_text('Codex native')
            expect(page.locator('#startup-controls')).to_contain_text(args.model)
            expect(page.locator('#startup-controls')).to_contain_text('effort '+args.effort)
            assert '#token' not in page.url
            denied = turn(page,False,args.output)
            assert not list(workspace.iterdir()), 'Denied task changed the workspace'
            accepted = turn(page,True,args.output)
            assert sorted(p.name for p in workspace.iterdir())==['calculator.py','test_calculator.py'], 'Unexpected file set'
            check = subprocess.run(['/usr/bin/python3','-B','-m','unittest','-v'],cwd=workspace,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=10)
            assert check.returncode==0 and b'Ran 3 tests' in check.stderr, 'Independent host tests failed; details omitted'
            current = snapshot(page,url)
            assert len(current['turns'])==2 and all(t['status']=='completed' for t in current['turns'])
            last = current['turns'][-1]['turn_id']
            metrics = current['turn_metrics'][last]
            assert metrics['text_updates']>0 and metrics['response_chars']>0
            tools = current['tool_activity'][last]['calls']
            assert tools and any(t['kind']=='edit' and t['status']=='completed' for t in tools)
            assert all(set(t)=={'ordinal','kind','status'} for t in tools)
            page.wait_for_timeout(1200)
            assert snapshot(page,url)['turn_metrics'][last]==metrics, 'Terminal metrics changed'
            page.screenshot(path=str(args.output/'codex-workspace-completed.png'),full_page=True)
            page.locator('.navigation [data-view="activity"]').click()
            expect(page.locator('#activity-view')).to_be_visible()
            expect(page.frame_locator('#timeline').get_by_text('Session not closed',exact=True)).to_be_visible()
            page.screenshot(path=str(args.output/'codex-workspace-history.png'),full_page=True)
            page.locator('.navigation [data-view="workspace"]').click()
            page.set_viewport_size({'width':390,'height':844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'Mobile overflow'
            expect(page.locator('#conversation-submit')).to_be_visible()
            page.screenshot(path=str(args.output/'codex-workspace-mobile.png'),full_page=True)
            assert not errors, 'Browser JavaScript error'
            with sqlite3.connect(database) as connection:
                receipts = [json.loads(row[0]) for row in connection.execute('select data from events where kind=?', ('"Permission"',))]
            assert any(r['outcome']=='rejected_once' for r in receipts) and any(r['outcome']=='allowed_once' for r in receipts)
            assert all(r['authorization']=='one_shot' for r in receipts)
            assert all(not any(k in r for k in ['command','path','rawInput','item','prompt','response']) for r in receipts)
            evidence={'native_version':'0.159.2','model_reported':args.model,'effort_reported':args.effort,
                'deny_task':denied,'allow_task':accepted,'host_tests_passed':3,'turn_metrics':metrics,
                'tool_categories':[t['kind'] for t in tools],'redacted_one_shot_receipts':len(receipts),
                'desktop_mobile_checked':True,'automatic_routing':False}
            (args.output/'evidence.json').write_text(json.dumps(evidence,indent=2)+'\n')
            print('PASS: real Codex browser conversation, deny/one-shot edit, independent tests, metrics, tool activity, history and mobile layout')
        finally: browser.close()


def main():
    cli=parser(); args=cli.parse_args()
    if not args.live_account:
        cli.error('--live-account is required; this check uses real account quota')
    for name in ['urdia','codex','chromium']:
        path=getattr(args,name).resolve(strict=True)
        if not path.is_file() or not os.access(path,os.X_OK): cli.error('Expected executable')
        setattr(args,name,path)
    args.output=args.output.resolve()
    try: run(args)
    except Exception:
        print('FAIL: real Codex browser check; private diagnostics omitted',file=sys.stderr)
        return 1
    return 0


if __name__=='__main__': raise SystemExit(main())
