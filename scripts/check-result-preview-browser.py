#!/usr/bin/env python3
"""Optional offline acceptance of interactive result previews using Launchboard.
The ACP completion is a fixture, not a new model run. No credentials are used.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import signal
import subprocess
import tempfile
import time
from urllib.parse import urlsplit
from playwright.sync_api import sync_playwright, expect


@contextmanager
def session(args):
    with tempfile.TemporaryDirectory(prefix='urdia-result-preview-') as raw:
        root = Path(raw); workspace = root / 'launchboard'; workspace.mkdir()
        for file in (Path(__file__).resolve().parent.parent / 'examples/launchboard/app').iterdir():
            (workspace / file.name).write_bytes(file.read_bytes())
        config = root / 'providers.toml'
        config.write_text('[[provider]]\nid="offline"\nname="Preview test fixture (not AI)"\ncommand=' + json.dumps(str(args.fake_agent)) + '\nargs=["--stream"]\n')
        with tempfile.TemporaryFile() as errors:
            p = subprocess.Popen([str(args.urdia), '--config', str(config), 'chat', '--provider', 'offline', '--workspace', str(workspace), '--database', str(root/'events.sqlite'), '--permissions-ui', '--bind', '127.0.0.1:0'], env={'PATH':'/usr/bin:/bin','HOME':str(root),'LC_ALL':'C.UTF-8'}, stdout=subprocess.PIPE, stderr=errors, start_new_session=True)
            try:
                data = b''; found = None; deadline = time.monotonic() + 25
                with selectors.DefaultSelector() as poll:
                    poll.register(p.stdout, selectors.EVENT_READ)
                    while time.monotonic() < deadline and not found:
                        for fd, _ in poll.select(.1): data += os.read(fd.fileobj.fileno(), 4096)
                        found = re.search(rb'http://127\.0\.0\.1:\d+/#token=[0-9a-f]{64}', data)
                        if p.poll() is not None: break
                assert found, 'fixture startup failed; private diagnostics omitted'
                yield found.group().decode(), workspace
            finally:
                if p.poll() is None:
                    os.killpg(p.pid, signal.SIGINT)
                    try: p.wait(timeout=8)
                    except subprocess.TimeoutExpired: os.killpg(p.pid, signal.SIGKILL); p.wait(timeout=5)
                p.stdout.close()


def run(args):
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    results = []
    with sync_playwright() as driver:
        browser = driver.chromium.launch(executable_path=str(args.chromium), chromium_sandbox=True)
        try:
            for width in [1440, 390, 320]:
                with session(args) as (url, workspace):
                    context = browser.new_context(viewport={'width':width,'height':950}, reduced_motion='reduce')
                    page = context.new_page(); errors=[]; page.on('pageerror', lambda _:errors.append(True))
                    page.goto(url); page.wait_for_selector('#conversation-input')
                    base, token = url.split('#token=')
                    def api(path, body=None):
                        headers={'Authorization':'Bearer '+token}
                        return page.request.get(base+path,headers=headers) if body is None else page.request.post(base+path,headers=headers,data=body)
                    expect(page.locator('#result-preview-panel')).to_be_hidden()
                    expect(page.locator('#result-preview-start')).to_be_disabled()
                    assert page.request.post(base+'api/conversation/result',data={}).status == 401
                    page.locator('#conversation-input').fill('Preview fixture only: no model or project edits.')
                    page.locator('#conversation-submit').click();expect(page.locator('#conversation-state')).to_have_text('completed',timeout=15000)
                    page.locator('#outcome-preview').click()
                    expect(page.locator('#result-preview-start')).to_be_disabled()
                    page.locator('#result-preview-consent').check();page.locator('#result-preview-start').click()
                    expect(page.locator('#result-preview-open')).to_be_visible(timeout=15000)
                    job=api('api/conversation').json()['result_preview']['latest']; original_url=job['url']
                    assert urlsplit(original_url).hostname != urlsplit(base).hostname
                    assert job['file_count']==3 and re.fullmatch('[0-9a-f]{64}',job['fingerprint'])
                    assert not page.locator('#result-preview-consent').is_checked()
                    expect(page.locator('#result-preview-open')).to_have_attribute('rel','noopener noreferrer')
                    page.evaluate("localStorage.setItem('urdia-parent-sentinel','PRIVATE')")
                    with context.expect_page() as opening:page.locator('#result-preview-open').click()
                    app=opening.value;app.wait_for_load_state();expect(app.locator('.task-card')).to_have_count(6)
                    assert app.evaluate("window.opener===null && sessionStorage.getItem('urdia-token')===null && localStorage.getItem('urdia-parent-sentinel')===null")
                    app.locator('#new-task').click();app.locator('#task-title').fill('Preview works');app.locator('#save-task').click();expect(app.locator('.task-card')).to_have_count(7)
                    app.reload();expect(app.locator('.task-card')).to_have_count(7)
                    app.locator('[data-task-id="discover"] [data-action="next"]').click()
                    expect(app.locator('[data-lane="progress"] [data-task-id="discover"]')).to_have_count(1)
                    with app.expect_download() as download:app.locator('#export-board').click()
                    assert len(json.loads(Path(download.value.path()).read_text()))==7
                    app.once('dialog',lambda d:d.dismiss());app.locator('#reset-board').click();expect(app.locator('.task-card')).to_have_count(7)
                    assert app.evaluate('document.documentElement.scrollWidth<=innerWidth')
                    app.screenshot(path=str(args.output/f'app-{width}.png'),full_page=True)
                    # Immutable bytes: editing the workspace does not change this preview.
                    original=(workspace/'app.js').read_bytes();(workspace/'app.js').write_bytes(original+b'\n// new captured version\n')
                    origin=urlsplit(original_url); numeric=f'http://127.0.0.1:{origin.port}'
                    served=page.request.get(numeric+'/app.js',headers={'Host':origin.netloc})
                    assert served.status==200 and served.body()==original
                    assert page.request.get(numeric+'/',headers={'Host':'attacker.invalid'}).status==404
                    assert page.request.get(numeric+'/api/conversation',headers={'Host':origin.netloc}).status==404
                    replay=api('api/conversation/result',{'request_id':job['request_id'],'turn_id':job['turn_id'],'confirm_execution':True}).json()['result_preview']['latest']
                    assert replay['url']==original_url and replay['fingerprint']==job['fingerprint']
                    page.locator('#conversation-input').fill('Another fixture completion, no model call.');page.locator('#conversation-submit').click();expect(page.locator('#conversation-state')).to_have_text('completed',timeout=15000)
                    expect(page.locator('#result-preview-status')).to_contain_text('Earlier task',timeout=10000)
                    # Each preview has a fresh browser origin, including after a new task.
                    app.evaluate("document.cookie='preview_origin_sentinel=PRIVATE; Domain=localhost; Path=/'")
                    page.locator('#outcome-preview').click()
                    page.locator('#result-preview-consent').check();page.locator('#result-preview-start').click()
                    expect(page.locator('#result-preview-open')).not_to_have_attribute('href',original_url,timeout=10000)
                    expect(page.locator('#result-preview-open')).to_be_visible()
                    with context.expect_page() as opening:page.locator('#result-preview-open').click()
                    fresh=opening.value;fresh.wait_for_load_state();expect(fresh.locator('.task-card')).to_have_count(6)
                    assert fresh.evaluate("localStorage.getItem('urdia-launchboard-v1')!==null && !document.cookie.includes('preview_origin_sentinel')")
                    current=api('api/conversation').json()['result_preview']['latest'];assert current['fingerprint']!=job['fingerprint']
                    assert api('api/conversation/result',{'request_id':job['request_id'],'turn_id':job['turn_id'],'confirm_execution':True}).status==409
                    page.screenshot(path=str(args.output/f'host-{width}.png'),full_page=True)
                    # Display fixture only: retain the real host lease while a later task looks active.
                    def active_display(route):
                        response=route.fetch(); data=response.json()
                        data['active_turn_id']=data['turns'][-1]['turn_id']; data['turns'][-1]['status']='running'
                        route.fulfill(response=response,json=data)
                    page.route('**/api/conversation',active_display)
                    expect(page.locator('#active-preview-stop')).to_be_visible(timeout=10000)
                    assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
                    page.locator('#active-preview-stop').click()
                    expect(page.locator('#result-preview-status')).to_contain_text('stopped',timeout=10000)
                    page.unroute('**/api/conversation',active_display)
                    expect(page.locator('#conversation-state')).to_have_text('completed',timeout=10000)
                    expect(page.locator('#result-preview-open')).to_be_hidden()
                    terminal=api('api/conversation/result',{'request_id':current['request_id'],'turn_id':current['turn_id'],'confirm_execution':True}).json()['result_preview']['latest'];assert terminal['status']=='stopped' and 'url' not in terminal
                    assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
                    assert not errors
                    results.append({'width':width,'completed_fixture_not_new_AI':True,'consent_and_separate_origin':True,'launchboard_create_reload_move_export':True,'root_token_and_storage_not_shared':True,'fresh_origin_cookie_storage_separation':True,'immutable_replay_and_older_task_label':True,'stop_is_terminal':True})
                    context.close()
        finally:browser.close()
    (args.output/'evidence.json').write_text(json.dumps({'checks':results,'model_calls':0},indent=2)+'\n')
    print(json.dumps(results))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['urdia','fake-agent','chromium','output']:parser.add_argument('--'+name,type=Path,required=True)
    run(parser.parse_args())
