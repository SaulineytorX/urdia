#!/usr/bin/env python3
"""Optional offline browser acceptance. Requires Playwright and local Chromium.

No credentials, external agents, browser traces, or provider APIs are used.
Only sanitized screenshots are written to an explicitly supplied directory.
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
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import time
import uuid

from playwright.sync_api import sync_playwright, expect


@contextmanager
def session(binary, fake, flags, trusted=False, advice=False, profile=False, profile_advice=False, recommended=False, editor=False, catalog_path=None, agent_path=None, primary=None, verification=None, recovery_fixture=False):
    with tempfile.TemporaryDirectory(prefix="urdia-browser-offline-") as raw:
        root = Path(raw)
        workspace = root / "Example project"
        workspace.mkdir()
        if verification:
            (workspace / "calculator.py").write_text("def add(a, b): return " + ("5" if verification == "bad" else "a + b") + "\n")
            suite = "import unittest\nfrom calculator import add\nclass T(unittest.TestCase):\n def test_x(self): self.assertEqual(add(2, 3), 5)\n"
            if verification.startswith("five"):
                fixtures = runpy.run_path(str(Path(__file__).resolve().parent / "tests/test_verify_calculator.py"))
                source = fixtures["FIVE_GOOD"]
                if verification == "five-bad": source = source.replace("operator.sub", "operator.add")
                (workspace / "calculator.py").write_text(source)
                suite = fixtures["FIVE_TESTS"]
            if verification == "hang": suite = "import time\ntime.sleep(60)\n"
            (workspace / "test_calculator.py").write_text(suite)
        if verification and verification.startswith("python-"):
            (workspace / "calculator.py").unlink();(workspace / "test_calculator.py").unlink()
            (workspace / "tax.py").write_text("def tax(amount): return amount * 0.16\n")
            source="import unittest\nfrom tax import tax\nclass T(unittest.TestCase):\n def test_tax(self): self.assertAlmostEqual(tax(100),16)\n"
            if verification=="python-empty": source="import unittest\n"
            if verification=="python-fail": source=source.replace("16)","99)")
            if verification=="python-hang": source="import time\ntime.sleep(60)\n"
            (workspace / "test_tax.py").write_text(source)
        config = root / "providers.toml"
        if profile:
            flags.append("--controls")
        config.write_text('[[provider]]\nid = "offline"\nname = "Offline reference agent (not AI)"\ncommand = '
                          + json.dumps(str(fake)) + '\nargs = ' + json.dumps(flags) + '\n')
        if recovery_fixture:
            launcher = root / "recovery-agent.py"
            counter = root / "recovery-starts"
            launcher.write_text("import os,sys\nfrom pathlib import Path\np=Path(" + repr(str(counter)) + ")\nn=int(p.read_text())+1 if p.exists() else 1\np.write_text(str(n))\nif n==2: sys.exit(1)\nf=" + repr(str(fake)) + "\nos.execv(f,[f]+(['--failure-auth'] if n==1 else []))\n")
            config.write_text('[[provider]]\nid = "offline"\nname = "Offline recovery agent (not AI)"\ncommand = "/usr/bin/python3"\nargs = '+json.dumps(["-I","-B",str(launcher)])+'\n')
        if profile:
            config.write_text('default_profile = "backend"\n' + config.read_text() +
                '\n[[profile]]\nid = "backend"\nname = "Offline backend profile"\nprovider = "offline"\nagent = "Reference ACP agent"\ninference_provider = "Offline (not AI)"\nmodel = "<img src=x onerror=alert(1)>"\nsession_controls = { model = "model-b(openai)", effort = "none" }\n')
        if recommended:
            config.write_text(config.read_text() + '\n[[provider]]\nid = "selected-launch"\nname = "Recommended reference agent"\ncommand = ' + json.dumps(str(fake)) +
                '\nargs = ["--stream", "--controls", "--request-permission"]\n[[profile]]\nid = "ahead"\nname = "Recommended calculator profile"\nprovider = "selected-launch"\nagent = "Reference ACP agent"\ninference_provider = "Offline (not AI)"\nmodel = "Model A"\neffort = "Medium"\nsession_controls = { model = "model-a(openai)", effort = "medium" }\n')
        store = Path(catalog_path) if catalog_path else root / "profiles.toml"
        with tempfile.TemporaryFile() as errors:
            process = subprocess.Popen(
                [str(binary), "--config", str(config), "chat", *(["--provider",primary] if primary else [] if profile else ["--provider", "offline"]),
                 "--workspace", str(workspace), "--database", str(root / "events.sqlite"),
                 "--permissions-ui", "--bind", "127.0.0.1:0", *(["--verification-ui"] if verification else []), *(["--profile-store",str(store)] if editor else []), *(["--agent-store",str(agent_path)] if agent_path else []), *(["--trusted-session"] if trusted else []), *(["--advisory-ui"] if advice else []), *(["--advisory-profiles"] if profile_advice else [])],
                env={"PATH": "/usr/bin:/bin", "HOME": str(root), "LC_ALL": "C"},
                stdout=subprocess.PIPE, stderr=errors, start_new_session=True)
            try:
                ready = b""
                match = None
                deadline = time.monotonic() + 15
                with selectors.DefaultSelector() as selector:
                    selector.register(process.stdout, selectors.EVENT_READ)
                    while time.monotonic() < deadline and not match:
                        for key, _ in selector.select(timeout=0.1):
                            ready += os.read(key.fileobj.fileno(), 4096)
                        match = re.search(rb"http://127\.0\.0\.1:\d+/#token=[^\s]+", ready)
                        if process.poll() is not None:
                            break
                assert match, "offline conversation did not start (private logs omitted)"
                yield match.group().decode()
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=5)
                process.stdout.close()


@contextmanager
def local_laya_fixture():
    calls = []
    key = "LAYA_BROWSER_PRIVATE_SENTINEL"
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_): pass
        def do_POST(self):
            if self.path != "/v1/systemone" or self.headers.get("Authorization") != "Bearer " + key:
                self.send_response(401); self.end_headers(); return
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            assert body['model'] == 'english'
            criteria = body['questions']['route']['criteria']
            selected = next(candidate for candidate in criteria if candidate != '__urdia_escalate__')
            calls.append(selected)
            raw = json.dumps({'model':'laya-rl-agent','answers':{'route':{'type':'choice','choice':selected,
                'probabilities':{candidate:float(candidate==selected) for candidate in criteria},'confidence':.99}},
                'routing':{'model':'english'},'usage':{'input_tokens':10,'output_tokens':0}}).encode()
            self.send_response(200); self.send_header('Content-Type','application/json')
            self.send_header('Content-Length',str(len(raw))); self.end_headers(); self.wfile.write(raw)
    server = ThreadingHTTPServer(('127.0.0.1',0),Handler)
    thread = threading.Thread(target=server.serve_forever,daemon=True); thread.start()
    try: yield 'http://127.0.0.1:'+str(server.server_port), key, calls
    finally: server.shutdown(); server.server_close(); thread.join(timeout=2)


def check_jev_settings_browser(browser, args):
    # Frontend contract fixture only. Actual host/probe/vault transitions have Rust tests.
    with session(args.urdia, args.fake_agent, ['--stream']) as url:
        page = browser.new_page(viewport={'width':1440,'height':1100})
        errors=[];page.on('pageerror',lambda error:errors.append(str(error)))
        page.goto(url);page.locator('.navigation [data-view="settings"]').click()
        expect(page.locator('#jev-form')).to_be_visible()
        # This server has no OS vault; session-only must remain available.
        expect(page.locator('#jev-state')).to_contain_text('Not connected')
        expect(page.locator('#jev-connect')).to_be_enabled()
        view={'status':'disconnected','remembered':False,'storage_kind':'secret_service'}
        saved={'value':None};calls=[]
        def metadata(route):
            response=route.fetch();body=response.json();body['jev_connection']=dict(view)
            for engine in body['engines']:
                if engine['id']=='jev':engine['configured']=view['status']=='connected'
            route.fulfill(response=response,json=body)
        def configure(route):
            body=route.request.post_data_json;action=body['action'];calls.append(action)
            if action in ('connect','check','restore'):
                assert body['allow_remote'] is True;uuid.UUID(body['request_id'])
            if action=='connect':
                assert body['api_key']=='JEV_BROWSER_TEST_SENTINEL';assert body['remember'] is True
                saved['value']=body['api_key'];view.update(status='connected',remembered=True)
            elif action=='disconnect':view.update(status='disconnected',remembered=False)
            elif action=='restore':
                assert saved['value']=='JEV_BROWSER_TEST_SENTINEL';assert 'api_key' not in body
                view.update(status='connected',remembered=True)
            elif action=='forget':saved['value']=None;view.update(status='disconnected',remembered=False)
            route.fulfill(json=dict(view))
        page.route('**/api/advisory',metadata);page.route('**/api/advisory/jev',configure)
        expect(page.locator('#jev-remember')).to_be_enabled(timeout=15000)
        page.locator('#jev-key').fill('JEV_BROWSER_TEST_SENTINEL');page.locator('#jev-remember').check()
        page.locator('#jev-connect').click();assert calls==[]
        page.locator('#jev-remote').check();page.locator('#jev-connect').click()
        expect(page.locator('#jev-state')).to_contain_text('key remembered')
        expect(page.locator('#jev-key')).to_have_value('')
        assert 'JEV_BROWSER_TEST_SENTINEL' not in page.content()
        assert 'JEV_BROWSER_TEST_SENTINEL' not in page.evaluate('JSON.stringify(localStorage)+JSON.stringify(sessionStorage)')
        assert page.locator('.conversation-turn').count()==0
        page.locator('#jev-disconnect').click();expect(page.locator('#jev-state')).to_contain_text('Not connected')
        page.locator('#jev-restore').click();expect(page.locator('#jev-state')).to_contain_text('key remembered')
        page.locator('#jev-forget').click();expect(page.locator('#jev-state')).to_contain_text('Not connected');assert saved['value'] is None
        page.set_viewport_size({'width':390,'height':844});assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
        page.screenshot(path=str(args.output/'jev-settings-mobile.png'),full_page=True)
        page.set_viewport_size({'width':1440,'height':1100});page.screenshot(path=str(args.output/'jev-settings.png'),full_page=True)
        assert calls==['connect','disconnect','restore','forget'];assert not errors,'browser script failed (details omitted)'
        page.close()


def check_laya_browser(browser, args):
    with local_laya_fixture() as (endpoint, key, calls), session(args.urdia,args.fake_agent,['--stream']) as url:
        page = browser.new_page(viewport={'width':1440,'height':1100})
        page.goto(url)
        page.locator('.navigation [data-view="settings"]').click()
        expect(page.locator('#laya-form')).to_be_visible()
        expect(page.locator('#laya-state')).to_contain_text('Not connected')
        assert calls == []
        page.locator('#laya-endpoint').fill(endpoint)
        page.locator('#laya-key').fill(key)
        page.locator('#laya-connect').click()
        expect(page.locator('#laya-state')).to_contain_text('Connected',timeout=15000)
        expect(page.locator('#laya-key')).to_have_value('')
        expect(page.locator('#advisory-engine')).to_have_value('laya',timeout=15000)
        assert key not in page.content()
        assert key not in page.evaluate('JSON.stringify(localStorage)+JSON.stringify(sessionStorage)')
        assert calls == ['connection_check']
        page.locator('#advisory-objective').fill('Review the example project')
        page.locator('#advisory-submit').click()
        expect(page.locator('#advisory-result')).to_contain_text('Recommended agent: offline',timeout=15000)
        assert page.locator('.conversation-turn').count()==0
        assert len(calls)==2
        # A failed credential replacement keeps the tested connection and existing key.
        page.locator('#laya-key').fill('WRONG_SENTINEL')
        page.locator('#laya-connect').click()
        expect(page.locator('#laya-message')).to_contain_text('Could not test Laya',timeout=15000)
        expect(page.locator('#laya-state')).to_contain_text('Connected')
        expect(page.locator('#laya-key')).to_have_value('')
        # Editing an address must survive polling and moving focus to the key field.
        page.locator('#laya-endpoint').fill('http://127.0.0.1:12345')
        page.locator('#laya-key').focus(); page.wait_for_timeout(1100)
        expect(page.locator('#laya-endpoint')).to_have_value('http://127.0.0.1:12345')
        page.locator('#laya-check').click()
        expect(page.locator('#laya-message')).to_contain_text('Connection tested',timeout=15000)
        assert len(calls)==3
        headers={'Authorization':'Bearer '+url.split('#token=',1)[1]}
        response=page.request.get(url.split('#',1)[0]+'api/advisory',headers=headers)
        assert response.ok and key not in response.text() and 'WRONG_SENTINEL' not in response.text()
        page.locator('#laya-endpoint').fill(endpoint)
        page.screenshot(path=str(args.output/'laya-connected.png'),full_page=True)
        page.set_viewport_size({'width':390,'height':844})
        assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
        page.locator('#laya-form').scroll_into_view_if_needed()
        page.screenshot(path=str(args.output/'laya-connected-mobile.png'),full_page=True)
        page.locator('#laya-disconnect').click()
        expect(page.locator('#laya-state')).to_contain_text('Not connected',timeout=15000)
        expect(page.locator('#advisory-submit')).to_be_disabled()
        response=page.request.get(url.split('#',1)[0]+'api/advisory',headers=headers)
        assert response.json()['laya_connection']=={'endpoint':None,'status':'disconnected'}
        page.close()


def check_agent_registration(browser,args):
    with tempfile.TemporaryDirectory(prefix="urdia-agent-onboarding-") as tmp:
        store=Path(tmp)/"agents.toml"; profiles=Path(tmp)/"profiles.toml"
        with session(args.urdia,args.fake_agent,["--stream"],editor=True,catalog_path=profiles,agent_path=store) as url:
            page=browser.new_page(viewport={"width":1440,"height":1100}); errors=[]
            page.on("pageerror",lambda error:errors.append(str(error)))
            page.goto(url);page.locator('.navigation [data-view="settings"]').click()
            page.locator('#agent-editor').evaluate('(e)=>e.open=true')
            page.locator('#agent-register-id').fill('new-agent');page.locator('#agent-register-name').fill('New coding agent')
            page.locator('#agent-register-command').fill(str(args.fake_agent));page.locator('#agent-register-args').fill('["--stream","--controls"]')
            page.wait_for_timeout(1200);expect(page.locator('#agent-register-id')).to_have_value('new-agent')
            page.locator('#agent-register-confirm').check();page.locator('#agent-register-save').click()
            expect(page.locator('#agent-register-save-status')).to_contain_text('Registered. Configure',timeout=15000)
            expect(page.locator('#agent-catalog-list')).to_contain_text('Pending restart')
            page.locator('.navigation [data-view="setup"]').click()
            page.locator('[data-setup-step="4"]').click()
            expect(page.locator('#setup-restart')).to_be_visible()
            page.locator('#setup-test-confirm').check()
            expect(page.locator('#setup-test-run')).to_be_disabled()
            page.locator('.navigation [data-view="settings"]').click()
            expect(page.locator('#agent-register-command')).to_have_value('')
            assert page.locator('.conversation-turn').count()==0
            expect(page.locator('#profile-edit-provider option[value="new-agent"]')).to_have_count(1)
            expect(page.locator('#profile-edit-provider')).to_contain_text('pending restart')
            headers={"Authorization":"Bearer "+url.split('#token=',1)[1]}
            snapshot=page.request.get(url.split('#',1)[0]+'api/agents',headers=headers).text()
            assert str(args.fake_agent) not in snapshot and '--controls' not in snapshot
            page.set_viewport_size({'width':390,'height':844});assert page.evaluate('document.documentElement.scrollWidth <= window.innerWidth')
            page.screenshot(path=str(args.output/'workspace-agent-onboarding-mobile.png'),full_page=True)
            assert not errors,'registration script error (private details omitted)'
            page.set_viewport_size({'width':1440,'height':1100})
            page.locator('#profile-editor').evaluate('(e)=>e.open=true')
            page.locator('#profile-advanced > summary').click()
            page.locator('#profile-edit-id').fill('new-backend');page.locator('#profile-edit-name').fill('New backend agent')
            page.locator('#profile-edit-provider').select_option('new-agent');page.locator('#profile-edit-agent').fill('Reference ACP agent')
            page.locator('#control-probe-start').click()
            expect(page.locator('#control-probe-status')).to_contain_text('Confirm the temporary')
            page.locator('#control-probe-confirm').check();page.locator('#control-probe-start').click()
            expect(page.locator('#control-probe-status')).to_contain_text('Agent-reported options',timeout=15000)
            page.locator('#control-model-options').select_option('model-b(openai)')
            expect(page.locator('#profile-edit-model-selector')).to_have_value('model-b(openai)')
            expect(page.locator('#control-effort-options')).to_be_disabled()
            page.locator('#control-probe-start').click()
            expect(page.locator('#control-probe-status')).to_contain_text('Agent-reported options for model model-b(openai)',timeout=15000)
            assert page.locator('#control-effort-options option').evaluate_all('(items)=>items.map(e=>e.value)')==['','default','none']
            page.locator('#control-effort-options').select_option('default')
            expect(page.locator('#profile-edit-effort-selector')).to_have_value('default')
            assert page.locator('.conversation-turn').count()==0
            page.screenshot(path=str(args.output/'workspace-control-discovery.png'),full_page=True)
            page.set_viewport_size({'width':390,'height':844});assert page.evaluate('document.documentElement.scrollWidth <= window.innerWidth')
            page.screenshot(path=str(args.output/'workspace-control-discovery-mobile.png'),full_page=True)
            page.locator('#profile-edit-default').check()
            page.locator('#profile-save').click();expect(page.locator('#profile-save-status')).to_contain_text('Saved. Restart',timeout=15000)
            page.screenshot(path=str(args.output/'workspace-agent-onboarding.png'),full_page=True)
            expect(page.locator('#agent-catalog-list')).to_contain_text('Pending restart')
            page.locator('.navigation [data-view="workspace"]').click()
            expect(page.locator('#agent-name')).to_have_text('Offline reference agent (not AI)')
            page.close()
        with session(args.urdia,args.fake_agent,["--stream"],profile=True,editor=True,catalog_path=profiles,agent_path=store) as url:
            page=browser.new_page();page.goto(url)
            expect(page.locator('#profile-name')).to_have_text('New backend agent')
            expect(page.locator('#startup-controls')).to_contain_text('model-b(openai)')
            assert page.locator('.conversation-turn').count()==0;page.close()


def check_profile_editor(browser,args):
    with tempfile.TemporaryDirectory(prefix="urdia-editor-restart-") as tmp:
        store=Path(tmp)/"profiles.toml"
        check_profile_edit_session(browser,args,store)
        with session(args.urdia,args.fake_agent,["--stream"],profile=True,editor=True,advice=True,profile_advice=True,catalog_path=store) as url:
            page=browser.new_page(viewport={"width":1440,"height":1100}); page.goto(url)
            expect(page.locator('#profile-name')).to_have_text('New backend settings')
            expect(page.locator('#startup-controls')).to_contain_text('model-a(openai)')
            page.locator('.navigation [data-view="settings"]').click()
            expect(page.locator('#catalog-status')).not_to_contain_text('Saved settings differ')
            assert page.locator('.conversation-turn').count()==0
            page.close()


def check_profile_edit_session(browser,args,store):
    with session(args.urdia,args.fake_agent,["--stream"],profile=True,editor=True,advice=True,profile_advice=True,catalog_path=store) as url:
        page=browser.new_page(viewport={"width":1440,"height":1100}); errors=[]
        page.on("pageerror",lambda error:errors.append(str(error)))
        page.goto(url); page.locator('.navigation [data-view="settings"]').click()
        expect(page.locator('#profile-catalog')).to_be_visible()
        expect(page.locator('#catalog-list')).to_contain_text('Offline backend profile')
        assert page.locator('#catalog-list img').count()==0
        page.locator('#profile-editor > summary').click()
        page.locator('#profile-edit-select').select_option('backend')
        page.locator('#profile-advanced > summary').click()
        expect(page.locator('#profile-edit-model-selector')).to_have_value('model-b(openai)')
        page.locator('#profile-edit-name').fill('New backend settings')
        page.locator('#profile-edit-model-selector').fill('model-a(openai)')
        page.locator('#profile-save').click()
        expect(page.locator('#profile-save-status')).to_contain_text('Saved.',timeout=15000)
        expect(page.locator('#catalog-status')).to_contain_text('Restart with the same profile store')
        expect(page.locator('#catalog-list')).to_contain_text('New backend settings')
        assert page.locator('.conversation-turn').count()==0
        page.locator('#profile-edit-name').fill('Unsaved draft')
        # A cooperating process holding the shared store lock blocks replacement.
        import fcntl
        with open(str(store)+'.lock','r+') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            page.locator('#profile-save').click()
            expect(page.locator('#profile-save-status')).to_contain_text('Settings changed',timeout=15000)
            fcntl.flock(lock,fcntl.LOCK_UN)
        page.wait_for_timeout(1200); expect(page.locator('#profile-edit-name')).to_have_value('Unsaved draft')
        page.locator('#profile-edit-model-selector').scroll_into_view_if_needed()
        page.screenshot(path=str(args.output/'workspace-profile-editor.png'),full_page=True)
        page.set_viewport_size({'width':390,'height':844})
        assert page.evaluate('document.documentElement.scrollWidth <= window.innerWidth')
        page.screenshot(path=str(args.output/'workspace-profile-editor-mobile.png'),full_page=True)
        page.locator('.navigation [data-view="workspace"]').click()
        expect(page.locator('#profile-name')).to_have_text('Offline backend profile')
        expect(page.locator('#startup-controls')).to_contain_text('model-b(openai)')
        page.locator('.navigation [data-view="settings"]').click()
        page.locator('#advisory-engine').select_option('deterministic')
        page.locator('#advisory-objective').fill('Review a calculator')
        page.locator('#advisory-submit').click()
        expect(page.locator('#advisory-result')).to_contain_text('Recommended profile: Offline backend profile',timeout=15000)
        assert not errors,'profile editor script failed (private diagnostics omitted)'
        page.close()


def check_automatic_jev_browser(browser, args):
    # UI names Jev; local Laya protocol fixture supplies typed decisions offline.
    # Actual host prepare/execute, model selectors and idempotency are exercised.
    for fallback in (False, True):
        with local_laya_fixture() as (endpoint, key, calls), session(args.urdia, args.fake_agent, ["--stream"], profile=True, recommended=True) as url:
            page=browser.new_page(viewport={"width":1440,"height":1100})
            errors=[];page.on('pageerror',lambda error:errors.append(str(error)))
            seen=[];executions=[];lost={'advice':False,'execute':False}
            def advice_route(route):
                if route.request.method=='GET':
                    response=route.fetch();body=response.json()
                    for engine in body['engines']:
                        if engine['id']=='jev':engine['configured']=True
                    if fallback and (body.get('latest') or {}).get('status')=='completed':
                        body['latest']['result']['selected_profile']=None
                        body['latest']['result']['fallback_reason']='no_selection'
                    route.fulfill(response=response,json=body)
                else:
                    body=route.request.post_data_json;assert body['engine']=='jev' and body['allow_remote'] is True
                    seen.append(body['request_id']);body['engine']='laya'
                    response=route.fetch(post_data=json.dumps(body))
                    if not fallback and not lost['advice']:
                        lost['advice']=True;route.abort()
                    else:route.fulfill(response=response)
            def execute_route(route):
                executions.append(route.request.post_data_json)
                response=route.fetch()
                if not lost['execute']:
                    lost['execute']=True;route.abort()
                else:route.fulfill(response=response)
            page.route('**/api/advisory',advice_route)
            page.route('**/api/advisory/execute',execute_route)
            page.goto(url)
            page.locator('.navigation [data-view="settings"]').click()
            page.locator('#laya-endpoint').fill(endpoint);page.locator('#laya-key').fill(key);page.locator('#laya-connect').click()
            expect(page.locator('#laya-state')).to_contain_text('Connected',timeout=15000)
            page.locator('.navigation [data-view="workspace"]').click()
            expect(page.locator('#task-routing-mode option[value="jev"]')).to_be_enabled(timeout=15000)
            page.locator('#task-routing-mode').select_option('jev')
            page.locator('#conversation-input').fill('Automatic typed profile task')
            page.locator('#conversation-submit').click()
            if fallback:
                expect(page.locator('#automatic-routing-status')).to_contain_text('did not select',timeout=15000)
                expect(page.locator('.conversation-turn')).to_have_count(0)
                expect(page.locator('#conversation-input')).to_have_value('Automatic typed profile task')
                assert not executions and len(calls)==2
            else:
                expect(page.locator('#automatic-routing-retry')).to_be_visible(timeout=15000)
                page.locator('#automatic-routing-retry').click()
                expect(page.locator('#automatic-routing-status')).to_contain_text('Delivery is uncertain',timeout=15000)
                expect(page.locator('#task-title')).to_have_text('Your decision is needed',timeout=15000)
                expect(page.locator('#automatic-routing-retry')).to_be_visible()
                page.locator('#automatic-routing-retry').click()
                expect(page.locator('#automatic-routing-status')).to_contain_text('Task admitted',timeout=15000)
                assert len(set(seen))==1 and len(calls)==2
                assert len(executions)==2 and executions[0]==executions[1]
                expect(page.locator('.conversation-turn')).to_have_count(1)
                page.locator('#permission-list button',has_text='Reject once').click()
                expect(page.locator('#task-title')).to_have_text('Task completed',timeout=15000)
                expect(page.locator('#conversation-history')).to_contain_text('model-a(openai)')
                page.screenshot(path=str(args.output/'jev-automatic-task.png'),full_page=True)
                page.set_viewport_size({'width':390,'height':844})
                assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
                page.screenshot(path=str(args.output/'jev-automatic-mobile.png'),full_page=True)
                page.reload();expect(page.locator('#task-routing-mode')).to_have_value('manual')
                expect(page.locator('.conversation-turn')).to_have_count(1)
            assert not errors,'automatic routing browser error (details omitted)'
            page.close()


def check_recommended_execution(browser, args):
    # Two dedicated launches: advice chooses a different model/profile from the default.
    with local_laya_fixture() as (endpoint, key, calls), session(args.urdia, args.fake_agent, ["--stream"], profile=True, advice=True, profile_advice=True, recommended=True) as url:
        page = browser.new_page(viewport={"width":1440,"height":1100})
        errors=[]; page.on("pageerror", lambda error: errors.append(str(error)))
        page.goto(url)
        page.locator('.navigation [data-view="settings"]').click()
        page.locator('#laya-endpoint').fill(endpoint)
        page.locator('#laya-key').fill(key)
        page.locator('#laya-connect').click()
        expect(page.locator('#laya-state')).to_contain_text('Connected',timeout=15000)
        page.locator('#advisory-engine').select_option('laya')
        original='Calculate the sum of five numbers'
        page.locator('#advisory-objective').fill(original)
        page.locator('#advisory-submit').click()
        expect(page.locator('#advisory-review-run')).to_be_visible(timeout=15000)
        assert page.locator('.conversation-turn').count()==0
        page.locator('#advisory-review-run').click()
        expect(page.locator('#advisory-run-preview')).to_contain_text('model-a(openai)')
        page.locator('#advisory-objective').fill('This edited draft must not be executed')
        expect(page.locator('#advisory-run-preview')).to_contain_text(original)
        assert page.locator('.conversation-turn').count()==0
        page.locator('#advisory-run-preview').scroll_into_view_if_needed()
        page.screenshot(path=str(args.output/'workspace-confirmed-review.png'),full_page=True)
        page.locator('#advisory-confirm-run').click()
        expect(page.locator('#task-title')).to_have_text('Your decision is needed',timeout=15000)
        expect(page.locator('#agent-name')).to_have_text('Recommended reference agent')
        expect(page.locator('#profile-selection')).to_have_text('Confirmed one-off task')
        expect(page.locator('#conversation-history')).to_contain_text(original)
        expect(page.locator('#conversation-history')).not_to_contain_text('edited draft')
        page.locator('#permission-list button',has_text='Reject once').click()
        expect(page.locator('#task-title')).to_have_text('Task completed',timeout=15000)
        expect(page.locator('#conversation-history')).to_contain_text('permission rejected')
        expect(page.locator('#conversation-history')).to_contain_text('model-a(openai)')
        expect(page.locator('#agent-name')).to_have_text('Offline reference agent (not AI)')
        page.screenshot(path=str(args.output/'workspace-confirmed-task.png'),full_page=True)
        page.set_viewport_size({'width':390,'height':844})
        assert page.evaluate('document.documentElement.scrollWidth <= window.innerWidth')
        page.screenshot(path=str(args.output/'workspace-confirmed-task-mobile.png'),full_page=True)
        # Subsequent ordinary task remains on the default launch/model and needs no permission fixture.
        page.locator('#conversation-input').fill('Ordinary default task')
        page.locator('#conversation-submit').click()
        expect(page.locator('#task-title')).to_have_text('Task completed',timeout=15000)
        expect(page.locator('.conversation-turn')).to_have_count(2)
        expect(page.locator('#profile-name')).to_have_text('Offline backend profile')
        assert not errors, 'browser script failed (details omitted)'
        page.close()


def check_step_advice_execution(browser, args):
    with local_laya_fixture() as (endpoint,key,calls), session(args.urdia,args.fake_agent,["--stream"],profile=True,advice=True,profile_advice=True,recommended=True) as url:
        page=browser.new_page(viewport={"width":1440,"height":1100})
        errors=[];page.on("pageerror",lambda error:errors.append(str(error)))
        page.goto(url)
        page.locator('.navigation [data-view="settings"]').click()
        page.locator('#laya-endpoint').fill(endpoint);page.locator('#laya-key').fill(key);page.locator('#laya-connect').click()
        expect(page.locator('#laya-state')).to_contain_text('Connected',timeout=15000)
        page.locator('.navigation [data-view="workspace"]').click()
        page.locator('#project-plan > summary').click()
        page.locator('#plan-objective').fill('Build a reviewed calculator')
        page.locator('#plan-manual > summary').click()
        for i in range(2):
            if i:page.locator('#plan-add').click()
            row=page.locator('#plan-builder fieldset').nth(i)
            row.locator('.plan-title').fill(['Backend','Front end'][i]);row.locator('.plan-scope').fill('Disposable workspace');row.locator('.plan-acceptance').fill('Review the result')
        page.locator('#plan-review').click()
        first=page.locator('#plan-board .plan-step').nth(0);second=page.locator('#plan-board .plan-step').nth(1)
        expect(first.get_by_role('button',name='Find a profile for this step')).to_be_enabled()
        expect(second.get_by_role('button',name='Find a profile for this step')).to_be_disabled()
        first.get_by_role('button',name='Find a profile for this step').click()
        expect(page.locator('#advisory-plan-context')).to_contain_text('Prepared project step 1')
        assert calls == ['connection_check'];assert page.locator('.conversation-turn').count()==0
        original=page.locator('#advisory-objective').input_value()
        assert 'Step 1/2: Backend' in original
        page.locator('#advisory-engine').select_option('laya');page.locator('#advisory-submit').click()
        expect(page.locator('#advisory-result')).to_contain_text('Advice for project step 1',timeout=15000)
        page.locator('#advisory-review-run').click()
        expect(page.locator('#advisory-run-preview')).to_contain_text('Bound project step 1')
        expect(page.locator('#advisory-run-preview')).to_contain_text('model-a(openai)')
        assert len(calls)==2;assert page.locator('.conversation-turn').count()==0
        # Changing an advice draft cannot alter the host-retained execution ticket.
        page.locator('#advisory-objective').fill('Ordinary edited advice draft')
        expect(page.locator('#advisory-plan-context')).to_contain_text('Ordinary task advice')
        dropped={'once':False}
        def lose_confirm_response(route):
            if not dropped['once']:dropped['once']=True;route.fetch();route.abort()
            else:route.continue_()
        page.route('**/api/advisory/execute',lose_confirm_response)
        page.locator('#advisory-confirm-run').click()
        expect(page.locator('#advisory-confirm-run')).to_be_enabled(timeout=15000)
        expect(page.locator('#advisory-run-preview')).to_contain_text('Could not confirm delivery')
        page.locator('#advisory-confirm-run').click()
        expect(page.locator('#task-title')).to_have_text('Your decision is needed',timeout=15000)
        expect(first).to_contain_text('Attempt 1')
        expect(page.locator('#conversation-history')).to_contain_text('Step 1/2: Backend')
        expect(page.locator('#conversation-history')).not_to_contain_text('Ordinary edited advice draft')
        expect(page.locator('#agent-name')).to_have_text('Recommended reference agent')
        page.locator('#permission-list button',has_text='Reject once').click()
        expect(first.locator('.plan-state')).to_have_text('Agent finished · review the result',timeout=15000)
        expect(page.locator('.conversation-turn')).to_have_count(1)
        expect(page.locator('#profile-name')).to_have_text('Offline backend profile')
        page.reload();expect(page.locator('#task-title')).to_have_text('Task completed',timeout=15000)
        page.locator('#project-plan > summary').click()
        expect(first).to_contain_text('Recommended reference agent')
        expect(first).to_contain_text('Profile: Recommended calculator profile')
        first.get_by_role('button',name='I reviewed this result').click()
        expect(page.locator('#plan-progress')).to_contain_text('1/2 operator-reviewed')
        page.screenshot(path=str(args.output/'project-step-advice.png'),full_page=True)
        page.set_viewport_size({'width':390,'height':844})
        assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
        page.screenshot(path=str(args.output/'project-step-advice-mobile.png'),full_page=True)
        page.set_viewport_size({'width':1440,'height':1100})
        second.get_by_role('button',name='Find a profile for this step').click()
        expect(page.locator('#advisory-plan-context')).to_contain_text('Prepared project step 2')
        page.locator('#advisory-engine').select_option('laya')
        page.locator('#advisory-submit').click()
        expect(page.locator('#advisory-result')).to_contain_text('Advice for project step 2',timeout=15000)
        page.locator('#advisory-review-run').click()
        expect(page.locator('#advisory-run-preview')).to_contain_text('Bound project step 2')
        # A changed host plan invalidates the ticket even if the preview is still visible.
        base=url.split('#',1)[0];headers={'Authorization':'Bearer '+url.split('#token=',1)[1]}
        snapshot=page.request.get(base+'api/conversation',headers=headers).json()['project_plan']
        response=page.request.post(base+'api/conversation/plan',headers=headers,data={'request_id':str(uuid.uuid4()),'revision':snapshot['revision'],'operation':{'kind':'clear','plan_id':snapshot['plan']['id']}})
        assert response.status==202
        page.locator('#advisory-confirm-run').click()
        expect(page.locator('#advisory-run-preview')).to_contain_text('review is no longer valid',timeout=15000)
        expect(page.locator('.conversation-turn')).to_have_count(1)
        assert len(calls)==3;assert not errors,'step advice browser error (details omitted)'
        page.close()


def check_profile_switching(browser, args):
    with session(args.urdia,args.fake_agent,["--stream"],profile=True,recommended=True) as url:
        page=browser.new_page(viewport={"width":1440,"height":1100})
        errors=[];page.on("pageerror",lambda error:errors.append(str(error)))
        page.goto(url)
        expect(page.locator("#next-profile-form")).to_be_visible()
        base=url.split("#",1)[0];headers={"Authorization":"Bearer "+url.split("#token=",1)[1]}
        def snapshot(): return page.request.get(base+"api/conversation",headers=headers).json()
        assert page.request.post(base+"api/conversation/profile",data={"revision":0,"profile_id":"ahead","confirm":True}).status == 401
        assert page.request.post(base+"api/conversation/profile",headers=headers,data={"revision":0,"profile_id":"ahead","confirm":False}).status == 400
        page.locator("#next-profile-select").select_option("ahead")
        expect(page.locator("#next-profile-preview")).to_contain_text("model-a(openai)")
        page.wait_for_timeout(700)
        expect(page.locator("#next-profile-select")).to_have_value("ahead")
        page.locator("#next-profile-apply").click()
        expect(page.locator("#next-profile-active")).to_contain_text("Recommended calculator profile")
        assert len(snapshot()["turns"])==0,"selection started a task"
        assert page.request.get(base+"api/timeline",headers=headers).text().count('<details class="session"') == 1
        assert snapshot()["profile_selection"]["revision"]==1
        assert page.request.post(base+"api/conversation/prompt",headers=headers,data={"turn_id":"a79b7915-df44-4c64-a994-64dd897172f9","text":"stale task","profile_revision":0}).status==409
        for index in range(2):
            page.locator("#conversation-input").fill(f"Manual profile test {index}")
            page.locator("#conversation-submit").click()
            expect(page.locator(".permission-card")).to_have_count(1,timeout=15000)
            expect(page.locator("#next-profile-select")).to_be_disabled()
            assert page.request.post(base+"api/conversation/profile",headers=headers,data={"revision":1,"profile_id":None,"confirm":True}).status==409
            page.locator(".permission-card button.reject").click()
            expect(page.locator("#task-title")).to_have_text("Task completed",timeout=15000)
            execution=snapshot()["turns"][-1]["execution"]
            assert execution["provider_id"]=="selected-launch"
            assert execution["startup_controls"]=={"model":"model-a(openai)","effort":"medium"}
            assert execution["profile"]["selection"]=="manual-next-task"
        assert page.request.get(base+"api/timeline",headers=headers).text().count('<details class="session"') == 3, "selected tasks did not use distinct sessions"
        page.screenshot(path=str(args.output/"workspace-manual-profile.png"),full_page=True)
        page.set_viewport_size({"width":390,"height":844})
        assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth")
        page.screenshot(path=str(args.output/"workspace-manual-profile-mobile.png"),full_page=True)
        # A second client changes selection between rendering and task delivery.
        def switch_before_delivery(route):
            response=page.request.post(base+"api/conversation/profile",headers=headers,data={"revision":1,"profile_id":None,"confirm":True})
            assert response.ok
            route.continue_()
        page.route("**/api/conversation/prompt",switch_before_delivery)
        page.locator("#conversation-input").fill("Stale UI task")
        page.locator("#conversation-submit").click()
        expect(page.locator("#next-profile-status")).to_contain_text("agent selection changed")
        assert len(snapshot()["turns"])==2,"stale UI task ran"
        page.unroute("**/api/conversation/prompt",switch_before_delivery)
        expect(page.locator("#next-profile-active")).to_contain_text("Original host session")
        expect(page.locator("#conversation-input")).to_have_value("Stale UI task")
        page.locator("#conversation-input").fill("Back to original")
        page.locator("#conversation-submit").click()
        expect(page.locator("#task-title")).to_have_text("Task completed",timeout=15000)
        data=snapshot()
        assert len(data["turns"])==3 and data["context"]["provider_id"]=="offline"
        assert data["turns"][-1]["original_session"] is True
        assert data["turns"][-1]["execution"] == data["context"], "original identity was not frozen at admission"
        assert page.request.get(base+"api/timeline",headers=headers).text().count('<details class="session"') == 3, "original session was not reused"
        assert not errors,"profile selection JavaScript failed"
        page.close()
    with session(args.urdia,args.fake_agent,["--stream"],profile=True,trusted=True) as url:
        page=browser.new_page();page.goto(url)
        expect(page.locator("#conversation")).to_be_visible()
        expect(page.locator("#next-profile-form")).to_be_hidden()
        assert page.request.post(url.split("#",1)[0]+"api/conversation/profile",headers={"Authorization":"Bearer "+url.split("#token=",1)[1]},data={"revision":0,"profile_id":"backend","confirm":True}).status==404
        page.close()


def check_setup_guide(browser, args):
    with session(args.urdia, args.fake_agent, ["--stream"], editor=True) as url:
        page = browser.new_page(viewport={"width":1440,"height":1000})
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.goto(url)
        expect(page.locator("#project-name")).to_have_text("Example project")
        expect(page.locator(".brand-mark")).to_be_visible()
        assert page.locator(".brand-mark").evaluate("n=>n.complete && n.naturalWidth > 0"), "brand asset did not load"
        expect(page.locator('link[rel="icon"]')).to_have_attribute("href","/brand.svg")
        expect(page.locator(".brand-credit")).to_contain_text("SaulineytorX")
        page.locator('[data-view="setup"]').first.click()
        expect(page.locator("#setup-context")).to_contain_text("Example project")
        for step in range(5):
            page.locator(f'[data-setup-step="{step}"]').click()
        expect(page.locator("#setup-test-run")).to_be_disabled()
        assert page.evaluate("new Set([...document.querySelectorAll('[id]')].map(n=>n.id)).size === document.querySelectorAll('[id]').length")
        token = url.split("#token=",1)[1]
        def snapshot():
            response = page.request.get(url.split("#",1)[0]+"api/conversation",headers={"Authorization":"Bearer "+token})
            assert response.ok
            return response.json()
        assert len(snapshot()["turns"]) == 0, "guide navigation started a task"
        page.locator('[data-setup-step="2"]').click()
        page.locator("#profile-editor").evaluate("n=>n.open=true")
        draft = page.locator("#profile-edit-name")
        draft.fill("setup-draft")
        page.locator('[data-view="settings"]').first.click()
        expect(draft).to_have_value("setup-draft")
        page.locator('[data-view="setup"]').first.click()
        expect(draft).to_have_value("setup-draft")
        page.locator('[data-setup-step="4"]').click()
        page.locator("#setup-test-confirm").check()
        expect(page.locator("#setup-test-run")).to_be_enabled()
        deliveries = []
        def deliver(route):
            deliveries.append(route.request.post_data_json["turn_id"])
            if len(deliveries) == 1:
                route.abort()
            else:
                route.continue_()
        page.route("**/api/conversation/prompt", deliver)
        page.locator("#setup-test-run").click()
        expect(page.locator("#setup-test-status")).to_contain_text("Delivery is uncertain")
        assert len(snapshot()["turns"]) == 0
        page.locator("#setup-test-run").click()
        expect(page.locator("#setup-test-status")).to_contain_text("Response needs review",timeout=15000)
        expect(page.locator("#setup-test-result")).to_contain_text("fake-agent:")
        expect(page.locator("#setup-test-metrics")).to_contain_text("Host elapsed:")
        expect(page.locator("#setup-test-run")).to_be_disabled()
        assert len(snapshot()["turns"]) == 1, "test dispatched more than once"
        assert len(deliveries) == 2 and deliveries[0] == deliveries[1], "retry changed test identity"
        # Exact-success display is exercised with a declared response fixture,
        # not claimed as real arithmetic inference by the offline echo agent.
        def arithmetic_fixture(route):
            response = route.fetch()
            data = response.json()
            data["turns"][0]["response"] = "20"
            route.fulfill(response=response,json=data)
        page.route("**/api/conversation", arithmetic_fixture)
        expect(page.locator("#setup-test-status")).to_contain_text("Arithmetic smoke check passed")
        page.unroute("**/api/conversation", arithmetic_fixture)
        expect(page.locator("#setup-test-status")).to_contain_text("Response needs review")
        page.screenshot(path=str(args.output/"setup-test-desktop.png"),full_page=True)
        page.set_viewport_size({"width":390,"height":844})
        assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth"), "setup guide overflows"
        page.screenshot(path=str(args.output/"setup-test-mobile.png"),full_page=True)
        assert not errors, "setup guide JavaScript failed"
        page.close()


def verify_calculator_browser(browser, args):
    for mode, expected in [("good", "passed"), ("bad", "failed"), ("hang", "cancelled"), ("five-good", "passed"), ("five-bad", "failed")]:
        with session(args.urdia, args.fake_agent, ["--stream"], verification=mode) as url:
            page = browser.new_page(viewport={"width": 1440, "height": 1100})
            page.goto(url)
            expect(page.locator("#verification-panel")).to_be_hidden()
            expect(page.locator("#verification-run")).to_be_disabled()
            page.locator("#conversation-input").fill("Offline calculator acceptance fixture; no AI edits.")
            page.locator("#conversation-submit").click()
            expect(page.locator("#conversation-state")).to_have_text("completed", timeout=15000)
            expect(page.locator("#verification-run")).to_be_disabled()
            page.locator("#outcome-check").click()
            if mode.startswith("five"):
                page.locator("#verification-contract").select_option("calculator-five-v1")
                page.locator("#verification-draft").click()
                expect(page.locator("#conversation-input")).to_have_value(re.compile(r"calculate\(numbers, operation\)"))
                expect(page.locator("#conversation-state")).to_have_text("completed")
                page.locator("#conversation-input").fill("Keep my draft")
                page.locator("#verification-draft").click()
                expect(page.locator("#conversation-input")).to_have_value("Keep my draft")
                page.locator("#verification-consent").check()
                page.locator("#verification-contract").select_option("calculator-add-v1")
                expect(page.locator("#verification-consent")).not_to_be_checked()
                page.locator("#verification-contract").select_option("calculator-five-v1")
            page.locator("#verification-consent").check()
            page.locator("#verification-run").click()
            if mode == "hang":
                expect(page.locator("#verification-status")).to_contain_text("Checking calculator files", timeout=15000)
                expect(page.locator("#conversation-submit")).to_be_disabled()
                page.locator("#verification-cancel").click()
            expect(page.locator("#verification-status")).to_contain_text("Checks cancelled" if mode == "hang" else "checks " + expected, timeout=15000)
            expect(page.locator("#conversation-submit")).to_be_enabled()
            if mode != "hang":
                expect(page.locator("#verification-results")).to_contain_text("Project unittest suite: passed")
                expect(page.locator("#verification-results")).to_contain_text(("Urdia’s 12 five-number test groups: " if mode.startswith("five") else "Urdia’s three add examples: ") + expected)
                expect(page.locator("#verification-results")).to_contain_text("file hashes stayed unchanged")
                expect(page.locator("#verification-results")).to_contain_text("1 test run · 0 failures · 0 errors · 0 skipped")
                if expected == "passed":
                    expect(page.locator("#verification-results")).to_contain_text(("12" if mode.startswith("five") else "3") + " tests run · 0 failures")
                if mode.startswith("five"):
                    page.locator("#verification-review summary").click()
                    page.locator("#verification-contract").select_option("calculator-add-v1")
                    expect(page.locator("#verification-status")).to_contain_text("Five-number arithmetic")
                    expect(page.locator("#verification-results")).to_contain_text("12 five-number test groups")
                page.screenshot(path=str(args.output / ("workspace-verification-" + mode + ".png")), full_page=True)
                page.set_viewport_size({"width":390,"height":844})
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
            page.locator("#conversation-input").fill("Next offline task.")
            page.locator("#conversation-submit").click()
            expect(page.locator("#conversation-state")).to_have_text("completed", timeout=15000)
            expect(page.locator("#verification-status")).to_contain_text("Historical result")
            page.close()


def verify_python_project_browser(browser,args):
    for mode,expected in [("python-good","Project tests passed"),("python-empty","Project tests did not pass"),("python-fail","Project tests did not pass"),("python-hang","Checks cancelled")]:
        with session(args.urdia,args.fake_agent,["--stream"],verification=mode) as url:
            page=browser.new_page(viewport={"width":1440,"height":1100});errors=[]
            page.on("pageerror",lambda _:errors.append(True));page.goto(url)
            expect(page.locator("#conversation-state")).to_have_text("ready",timeout=15000)
            expect(page.locator("#verification-run")).to_be_disabled()
            page.locator("#conversation-input").fill("Review this existing tax project; offline fixture")
            page.locator("#conversation-submit").click()
            expect(page.locator("#conversation-state")).to_have_text("completed",timeout=15000)
            page.locator("#outcome-check").click()
            page.locator("#verification-contract").select_option("python-unittest-v1")
            expect(page.locator("#verification-spec")).to_contain_text("no dependencies are installed",ignore_case=True)
            expect(page.locator("#verification-consent")).not_to_be_checked()
            page.locator("#verification-consent").check();page.locator("#verification-run").click()
            if mode=="python-hang":
                expect(page.locator("#verification-status")).to_contain_text("Running project tests",timeout=15000)
                page.locator("#verification-cancel").click()
            expect(page.locator("#verification-status")).to_contain_text(expected,timeout=15000)
            expect(page.locator(".conversation-turn")).to_have_count(1)
            if mode=="python-good":
                expect(page.locator("#verification-results")).to_contain_text("1 test run")
                expect(page.locator("#verification-results")).to_contain_text("2 Python source files matched")
                assert "host_contract" not in page.locator("#verification-results").inner_text()
                page.screenshot(path=str(args.output/"python-project-checks-desktop.png"),full_page=True)
                page.set_viewport_size({"width":390,"height":844})
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth+1")
                page.screenshot(path=str(args.output/"python-project-checks-mobile.png"),full_page=True)
            assert not errors
            page.close()


def verify_project_summary(browser,args):
    with session(args.urdia,args.fake_agent,["--stream"],verification="good") as url:
        page=browser.new_page(viewport={"width":1440,"height":1100})
        errors=[];page.on("pageerror",lambda _:errors.append(True));page.goto(url)
        expect(page.locator("#conversation-state")).to_have_text("ready",timeout=15000)
        expect(page.locator("#project-summary")).to_be_hidden()
        page.locator("#project-plan").evaluate("el=>el.open=true")
        page.locator("#plan-objective").fill("A checked project with explicit review")
        page.locator("#plan-manual").evaluate("el=>el.open=true")
        for index,title in enumerate(["<img src=x onerror=alert(1)> Build calculator","Review the result"]):
            if index:page.locator("#plan-add").click()
            row=page.locator("#plan-builder fieldset").nth(index)
            row.locator(".plan-title").fill(title);row.locator(".plan-scope").fill("Only calculator files")
            row.locator(".plan-acceptance").fill("Inspect the result and run checks explicitly")
        page.locator("#plan-review").click()
        expect(page.locator("#project-summary")).to_be_visible()
        expect(page.locator("#summary-completed")).to_have_text("0/2")
        expect(page.locator("#summary-elapsed")).to_contain_text("Not observed")
        first=page.locator("#plan-board .plan-step").nth(0);second=page.locator("#plan-board .plan-step").nth(1)
        first.get_by_role("button",name="Prepare step draft").click();page.locator("#conversation-submit").click()
        expect(page.locator("#conversation-state")).to_have_text("completed",timeout=15000)
        expect(page.locator("#summary-completed")).to_have_text("1/2")
        expect(page.locator("#summary-reviewed")).to_have_text("0/2")
        expect(page.locator("#summary-checked")).to_have_text("0/2")
        assert page.locator("#project-summary img").count()==0
        page.locator("#outcome-check").click()
        page.locator("#verification-consent").check();page.locator("#verification-run").click()
        expect(page.locator("#verification-status")).to_contain_text("Calculator checks passed",timeout=15000)
        expect(page.locator("#summary-checked")).to_have_text("1/2")
        expect(page.locator("#project-summary-steps li").nth(0)).to_contain_text("4 test executions")
        expect(page.locator("#summary-reviewed")).to_have_text("0/2")
        first.get_by_role("button",name="I reviewed this result").click()
        expect(page.locator("#summary-reviewed")).to_have_text("1/2")
        second.get_by_role("button",name="Prepare step draft").click();page.locator("#conversation-submit").click()
        expect(page.locator("#conversation-state")).to_have_text("completed",timeout=15000)
        expect(page.locator("#project-summary-title")).to_have_text("Work finished · review your results")
        expect(page.locator("#summary-checked")).to_have_text("1/2")
        expect(page.locator("#project-summary-steps li").nth(0)).to_contain_text("Historical: later task exists")
        expect(page.locator("#project-summary-steps li").nth(1)).to_contain_text("not run for this attempt")
        second.get_by_role("button",name="I reviewed this result").click()
        expect(page.locator("#project-summary-title")).to_have_text("All steps reviewed")
        expect(page.locator("#summary-reviewed")).to_have_text("2/2")
        page.locator("#project-summary-steps li").nth(0).get_by_role("button",name="View task").click()
        expect(page.locator(".conversation-turn").nth(0)).to_be_focused()
        page.wait_for_timeout(700)
        expect(page.locator(".conversation-turn").nth(0)).to_be_focused()
        page.locator("#project-summary").evaluate("el=>el.scrollIntoView({block:'start'})")
        page.screenshot(path=str(args.output/"project-summary-desktop.png"),full_page=True)
        page.set_viewport_size({"width":390,"height":844})
        assert page.evaluate("document.documentElement.scrollWidth <= innerWidth+1")
        page.screenshot(path=str(args.output/"project-summary-mobile.png"),full_page=True)
        assert len(page.locator(".conversation-turn").all())==2
        page.locator("#plan-clear").click()
        expect(page.locator("#project-summary")).to_be_hidden()
        expect(page.locator(".conversation-turn")).to_have_count(2)
        assert not errors
        page.close()


def verify_agent_reconnection(browser,args):
    with session(args.urdia,args.fake_agent,[],recovery_fixture=True) as url:
        page=browser.new_page(viewport={"width":1440,"height":1100})
        errors=[];page.on("pageerror",lambda _:errors.append(True));page.goto(url)
        expect(page.locator("#conversation-state")).to_have_text("ready",timeout=15000)
        base=url.split("#")[0];headers={"Authorization":"Bearer "+url.split("token=")[1]}
        expect(page.locator("#agent-reconnection")).to_be_hidden()
        # Inert host plan: no task or provider call is created by review/adoption.
        page.locator("#project-plan").evaluate("el => el.open=true")
        page.locator("#plan-objective").fill("Keep this reviewed project")
        page.locator("#plan-manual").evaluate("el => el.open=true")
        page.locator("#plan-builder .plan-title").fill("Check project")
        page.locator("#plan-builder .plan-scope").fill("Read only")
        page.locator("#plan-builder .plan-acceptance").fill("Report observations")
        page.locator("#plan-review").click()
        expect(page.locator("#plan-goal")).to_have_text("Keep this reviewed project")
        snapshot=page.request.get(base+"api/conversation",headers=headers).json()
        assert snapshot["project_plan"]["plan"] is not None
        step=page.locator("#plan-board .plan-step").first
        step.get_by_role("button",name="Prepare step draft").click()
        expect(page.locator("#conversation-input")).to_have_value(re.compile("Keep this reviewed project"))
        page.locator("#conversation-submit").click()
        expect(page.locator("#conversation-state")).to_have_text("failed",timeout=15000)
        saved_plan=page.request.get(base+"api/conversation",headers=headers).json()["project_plan"]
        expect(page.locator("#agent-reconnection")).to_be_visible()
        expect(page.locator("#reconnection-start")).to_be_disabled()
        # An explicit refusal remains visible after rendering/polling; it starts no session.
        page.route("**/api/conversation/reconnect",lambda route:route.fulfill(status=409,content_type="application/json",body='{"error":"reconnection_not_admitted"}'))
        page.locator("#reconnection-consent").check()
        page.locator("#reconnection-start").click()
        expect(page.locator("#reconnection-result")).to_contain_text("not admitted")
        expect(page.locator("#reconnection-consent")).not_to_be_checked()
        page.unroute("**/api/conversation/reconnect")
        page.locator("#conversation-input").fill("Preserve my next task draft")
        revision=page.request.get(base+"api/conversation",headers=headers).json()["reconnection"]["revision"]
        page.locator("#reconnection-consent").check()
        page.locator("#reconnection-start").click()
        expect(page.locator("#reconnection-result")).to_contain_text("cause is unknown",timeout=15000)
        expect(page.locator("#conversation-submit")).to_be_disabled()
        expect(page.locator("#reconnection-consent")).not_to_be_checked()
        expect(page.locator("#conversation-input")).to_have_value("Preserve my next task draft")
        assert page.request.get(base+"api/conversation",headers=headers).json()["project_plan"]==saved_plan
        page.locator("#reconnection-consent").check()
        page.locator("#reconnection-start").click()
        expect(page.locator("#reconnection-result")).to_contain_text("Agent reconnected",timeout=15000)
        after=page.request.get(base+"api/conversation",headers=headers).json()
        assert len(after["turns"])==1 and after["turns"][0]["status"]=="failed"
        assert after["reconnection"]["context_resets"]==1 and after["project_plan"]==saved_plan
        assert page.request.post(base+"api/conversation/reconnect",headers=headers,data={"revision":revision,"confirm_context_reset":True}).status==409
        page.screenshot(path=str(args.output/"agent-reconnected-desktop.png"),full_page=True)
        page.set_viewport_size({"width":390,"height":844})
        assert page.evaluate("document.documentElement.scrollWidth <= innerWidth + 1")
        page.screenshot(path=str(args.output/"agent-reconnected-mobile.png"),full_page=True)
        # Retry the linked failed stage only after explicitly preparing its reviewed context.
        page.locator("#conversation-input").fill("")
        step.get_by_role("button",name="Prepare retry draft").click()
        expect(page.locator("#conversation-input")).to_have_value(re.compile("Keep this reviewed project"))
        expect(page.locator("#conversation-input")).to_have_value(re.compile("Read only"))
        page.locator("#conversation-submit").click()
        expect(page.locator("#conversation-state")).to_have_text("completed",timeout=15000)
        expect(page.locator(".conversation-turn")).to_have_count(2)
        expect(step.locator(".plan-state")).to_have_text("Agent finished · review the result")
        expect(step.get_by_role("button",name="I reviewed this result")).to_be_enabled()
        final=page.request.get(base+"api/conversation",headers=headers).json()
        assert final["turns"][0]["status"]=="failed" and final["turns"][1]["status"]=="completed"
        assert final["project_plan"]["plan"]["steps"][0]["reviewed"] is False
        assert final["project_plan"]["plan"]["steps"][0]["attempts"]==2
        assert not errors
        page.close()


def verify_provider_failure_browser(browser,args):
    for flag,kind,phrase in [("--failure-auth","authentication","authentication problem"),
                            ("--failure-rate","rate_or_quota","usage or rate limit"),
                            ("--failure-model","model_unavailable","model is unavailable"),
                            ("--failure-connection","connection","connection failed"),
                            ("--failure-unknown","unknown","cause is unknown")]:
        with session(args.urdia,args.fake_agent,[flag]) as url:
            page=browser.new_page(viewport={"width":1440,"height":1100})
            errors=[];page.on("pageerror",lambda _:errors.append(True));page.goto(url)
            expect(page.locator("#conversation-state")).to_have_text("ready",timeout=15000)
            page.locator("#conversation-input").fill("Offline failure fixture; no model call")
            page.locator("#conversation-submit").click()
            expect(page.locator("#conversation-state")).to_have_text("failed",timeout=15000)
            expect(page.locator("#task-detail")).to_contain_text(phrase)
            expect(page.locator(".conversation-error")).to_contain_text(phrase)
            expect(page.locator("#conversation-submit")).to_be_enabled()
            expect(page.locator(".permission-card")).to_have_count(0)
            assert "PRIVATE_FAILURE_SENTINEL" not in page.content()
            assert not page.locator(".task-overview img,.conversation-error img").count()
            page.wait_for_timeout(700)
            expect(page.locator(".conversation-turn")).to_have_count(1)
            base=url.split("#",1)[0];headers={"Authorization":"Bearer "+url.split("#token=",1)[1]}
            snapshot=page.request.get(base+"api/conversation",headers=headers)
            assert "PRIVATE_FAILURE_SENTINEL" not in snapshot.text()
            assert len(snapshot.json()["turns"])==1 and snapshot.json()["active_turn_id"] is None
            page.screenshot(path=str(args.output/("provider-failure-"+kind+".png")),full_page=True)
            page.locator('.navigation [data-view="activity"]').click()
            expect(page.frame_locator("#timeline").get_by_text(re.compile(phrase))).to_be_visible()
            page.locator('.navigation [data-view="workspace"]').click()
            if kind=="authentication":
                page.set_viewport_size({"width":390,"height":844})
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth + 1")
                page.screenshot(path=str(args.output/"provider-failure-authentication-mobile.png"),full_page=True)
                # A second task requires an explicit new send; no automatic retry.
                page.locator("#conversation-input").fill("Explicit second fixture request")
                page.locator("#conversation-submit").click()
                expect(page.locator(".conversation-turn")).to_have_count(2)
                expect(page.locator("#conversation-state")).to_have_text("failed",timeout=15000)
                assert all(t["status"]=="failed" for t in page.request.get(base+"api/conversation",headers=headers).json()["turns"])
            assert not errors
            page.close()


def verify_plan_proposal_browser(browser, args):
    # The reference agent is not AI. A response fixture exercises parsing and review,
    # not semantic planning quality or actual model output.
    with session(args.urdia, args.fake_agent, ["--stream"]) as url:
        page=browser.new_page(viewport={"width":1440,"height":1100})
        errors=[]
        page.on("pageerror", lambda _: errors.append(True))
        page.goto(url)
        expect(page.locator("#conversation-state")).to_have_text("ready",timeout=15000)
        expect(page.locator("#plan-result")).to_be_hidden()
        page.locator("#project-plan > summary").click()
        page.locator("#plan-propose").click()
        expect(page.locator("#plan-note")).to_contain_text("objective first")
        page.locator("#plan-objective").fill("Build a five-number calculator")
        assert not page.locator("#plan-manual").evaluate("el => el.open")
        # Native invalid-field validation must reveal collapsed required inputs.
        assert not page.locator("#plan-form").evaluate("el => el.reportValidity()")
        assert page.locator("#plan-manual").evaluate("el => el.open")
        page.locator("#conversation-input").fill("Keep my existing message")
        page.locator("#plan-propose").click()
        expect(page.locator("#conversation-input")).to_have_value("Keep my existing message")
        page.locator("#conversation-input").fill("")
        page.locator("#plan-propose").click()
        expect(page.locator("#conversation-input")).to_have_value(re.compile("Prepare a proposed project plan only"))
        expect(page.locator("#conversation-state")).to_have_text("ready")
        expect(page.locator("#plan-result")).to_be_hidden()
        page.locator("#conversation-submit").click()
        expect(page.locator("#conversation-state")).to_have_text("completed",timeout=15000)
        page.locator("#plan-manual > summary").click()
        page.locator("#plan-builder .plan-title").fill("Preserve my outline")
        page.locator("#plan-proposal-review").click()
        expect(page.locator("#plan-note")).to_contain_text("not a valid current plan")
        expect(page.locator("#plan-builder .plan-title")).to_have_value("Preserve my outline")
        draft={"objective":"Build a five-number calculator", "steps":[
            {"title":"Implement arithmetic <img src=x onerror=alert(1)>","scope":"calculator.py only","acceptance":"Five numbers; four operations; invalid inputs rejected"},
            {"title":"Verify arithmetic","scope":"test_calculator.py only","acceptance":"Run unit tests; include zero divisors and preserved input"}]}
        def response_fixture(route):
            response=route.fetch();data=response.json()
            if data.get("turns") and data["turns"][-1]["status"]=="completed":
                data["turns"][-1]["response"]=json.dumps(draft)
            route.fulfill(response=response,json=data)
        page.route("**/api/conversation",response_fixture)
        page.reload()
        expect(page.locator("#conversation-state")).to_have_text("completed",timeout=15000)
        expect(page.locator("#plan-result")).to_be_visible()
        expect(page.locator("#plan-proposal-preview")).to_be_hidden()
        assert not page.locator("#project-plan").evaluate("el => el.open")
        page.screenshot(path=str(args.output/"planning-response-ready.png"),full_page=True)
        page.set_viewport_size({"width":390,"height":844})
        assert page.evaluate("document.documentElement.scrollWidth <= innerWidth + 1")
        page.screenshot(path=str(args.output/"planning-response-ready-mobile.png"),full_page=True)
        page.set_viewport_size({"width":1440,"height":1100})
        # Returning from the transcript requires an explicit click, never auto-adoption.
        page.locator("#plan-result-review").click()
        assert page.locator("#project-plan").evaluate("el => el.open")
        expect(page.locator("#plan-proposal-preview")).to_be_visible()
        assert not page.locator("#plan-proposal-preview img").count()
        expect(page.locator("#plan-board")).to_be_hidden()
        page.screenshot(path=str(args.output/"project-plan-proposal-preview.png"),full_page=True)
        page.set_viewport_size({"width":390,"height":844})
        assert page.evaluate("document.documentElement.scrollWidth <= innerWidth + 1")
        page.screenshot(path=str(args.output/"project-plan-proposal-preview-mobile.png"),full_page=True)
        page.set_viewport_size({"width":1440,"height":1100})
        # A newer explicit task invalidates the old source preview.
        page.locator("#conversation-input").fill("Another explicit planning request")
        page.locator("#conversation-submit").click()
        expect(page.locator("#conversation-state")).to_have_text("completed",timeout=15000)
        expect(page.locator("#plan-proposal-apply")).to_be_disabled()
        expect(page.locator("#plan-result")).to_be_hidden()
        page.locator("#plan-proposal-review").click()
        expect(page.locator("#plan-proposal-apply")).to_be_enabled()
        page.locator("#plan-proposal-apply").click()
        expect(page.locator("#plan-builder fieldset")).to_have_count(2)
        expect(page.locator("#plan-board")).to_be_hidden()
        expect(page.locator(".conversation-turn")).to_have_count(2)
        # Editable before acceptance, and no second task at acceptance.
        page.locator("#plan-builder .plan-title").nth(0).fill("Implement arithmetic")
        page.locator("#plan-review").click()
        expect(page.locator("#plan-board .plan-step")).to_have_count(2)
        expect(page.locator("#plan-result")).to_be_hidden()
        expect(page.locator(".conversation-turn")).to_have_count(2)
        expect(page.locator("#plan-progress")).to_contain_text("0/2 operator-reviewed")
        page.screenshot(path=str(args.output/"project-plan-proposal.png"),full_page=True)
        page.set_viewport_size({"width":390,"height":844})
        assert page.evaluate("document.documentElement.scrollWidth <= innerWidth + 1")
        page.screenshot(path=str(args.output/"project-plan-proposal-mobile.png"),full_page=True)
        assert not errors, "proposal browser error (private details omitted)"
        # Drain the polling response fixture before disposing its context.
        page.unroute_all(behavior="wait")
        page.close()


def verify_project_plan_browser(browser, args):
    with session(args.urdia, args.fake_agent, ["--stream"]) as url:
        page = browser.new_page(viewport={"width":1440,"height":1100})
        page.goto(url)
        expect(page.locator("#conversation-state")).to_have_text("ready", timeout=15000)
        page.locator("#project-plan > summary").click()
        page.locator("#plan-objective").fill("A reviewed calculator project <img src=x onerror=alert(1)>")
        page.locator("#plan-manual > summary").click()
        for i in range(2):
            if i: page.locator("#plan-add").click()
            row = page.locator("#plan-builder fieldset").nth(i)
            row.locator(".plan-title").fill(["Implement arithmetic", "Review the front end"][i])
            row.locator(".plan-scope").fill(["calculator.py and test_calculator.py", "Read-only review; do not edit files"][i])
            row.locator(".plan-acceptance").fill(["Five-number arithmetic contract passes", "Explain UI requirements and remaining work"][i])
        page.locator("#plan-review").click()
        expect(page.locator("#plan-board .plan-step")).to_have_count(2)
        expect(page.locator("#conversation-state")).to_have_text("ready")
        assert not page.locator("#project-plan img").count()
        first = page.locator("#plan-board .plan-step").nth(0)
        second = page.locator("#plan-board .plan-step").nth(1)
        expect(second.get_by_role("button",name="Prepare step draft")).to_be_disabled()
        page.locator("#conversation-input").fill("Keep this draft")
        first.get_by_role("button",name="Prepare step draft").click()
        expect(page.locator("#conversation-input")).to_have_value("Keep this draft")
        page.locator("#conversation-input").fill("")
        first.get_by_role("button",name="Prepare step draft").click()
        expect(page.locator("#conversation-input")).to_have_value(re.compile("Step 1/2: Implement arithmetic"))
        expect(page.locator("#conversation-state")).to_have_text("ready")
        # Drop the successful response after actual host admission. Polling must bind
        # the accepted UUID; retry uses the original task, never a second execution.
        dropped = {"done":False}
        def drop_response(route):
            if not dropped["done"]:
                dropped["done"] = True
                route.fetch(); route.abort()
            else: route.continue_()
        page.route("**/api/conversation/prompt", drop_response)
        page.locator("#conversation-submit").click()
        expect(first.locator(".plan-state")).to_have_text("Agent finished · review the result", timeout=15000)
        expect(page.locator(".conversation-turn")).to_have_count(1)
        expect(second.get_by_role("button",name="Prepare step draft")).to_be_disabled()
        # Resolve uncertainty by retrying the same prompt/UUID, then review explicitly.
        page.locator("#conversation-submit").click()
        expect(page.locator("#conversation-input")).to_have_value("")
        expect(page.locator(".conversation-turn")).to_have_count(1)
        first.get_by_role("button",name="I reviewed this result").click()
        expect(page.locator("#plan-progress")).to_contain_text("1/2 operator-reviewed")
        second.get_by_role("button",name="Prepare step draft").click()
        page.locator("#conversation-submit").click()
        expect(second.locator(".plan-state")).to_have_text("Agent finished · review the result", timeout=15000)
        expect(page.locator(".conversation-turn")).to_have_count(2)
        expect(first.locator(".plan-state")).to_contain_text("not independently verified")
        page.reload()
        expect(page.locator("#conversation-state")).to_have_text("completed", timeout=15000)
        page.locator("#project-plan > summary").click()
        expect(page.locator("#plan-board .plan-step")).to_have_count(2)
        expect(page.locator("#plan-progress")).to_contain_text("1/2 operator-reviewed")
        expect(second.locator(".plan-state")).to_have_text("Agent finished · review the result")
        expect(page.locator("#plan-goal")).to_have_text("A reviewed calculator project <img src=x onerror=alert(1)>")
        assert not page.locator("#project-plan img").count()
        page.locator("#project-plan").evaluate("el => el.scrollIntoView({block:'start'})")
        page.screenshot(path=str(args.output/"project-plan-progress.png"),full_page=True)
        page.set_viewport_size({"width":390,"height":844})
        assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
        page.locator("#project-plan").evaluate("el => el.scrollIntoView({block:'start'})")
        page.screenshot(path=str(args.output/"project-plan-mobile.png"),full_page=True)
        page.locator("#plan-clear").click()
        expect(page.locator("#plan-form")).to_be_visible()
        expect(page.locator(".conversation-turn")).to_have_count(2)
        page.locator("#plan-manual > summary").click()
        for _ in range(7): page.locator("#plan-add").click()
        expect(page.locator("#plan-add")).to_be_disabled()
        expect(page.locator("#plan-builder fieldset")).to_have_count(8)
        page.reload()
        expect(page.locator("#conversation-state")).to_have_text("completed", timeout=15000)
        page.locator("#project-plan > summary").click()
        expect(page.locator("#plan-builder fieldset")).to_have_count(1)
        expect(page.locator("#plan-objective")).to_have_value("")
        page.locator("#plan-objective").fill("An unlinked edited task")
        page.locator("#plan-manual > summary").click()
        page.locator(".plan-title").fill("Original reviewed step")
        page.locator(".plan-scope").fill("Read-only")
        page.locator(".plan-acceptance").fill("Operator inspects explanation")
        page.locator("#plan-review").click()
        page.locator("#plan-board").get_by_role("button",name="Prepare step draft").click()
        original = page.locator("#conversation-input").input_value()
        page.locator("#conversation-input").fill(original + " Edited draft.")
        page.locator("#conversation-submit").click()
        expect(page.locator("#conversation-state")).to_have_text("completed",timeout=15000)
        expect(page.locator(".conversation-turn")).to_have_count(3)
        expect(page.locator(".conversation-turn").last.locator(".conversation-turn-heading span")).to_have_text("completed", timeout=15000)
        expect(page.locator("#plan-board").get_by_role("button", name="Prepare step draft")).to_be_enabled()
        expect(page.locator(".plan-state")).to_have_text("Ready to prepare")
        page.evaluate("window.planNode = document.querySelector('.plan-step')")
        page.wait_for_timeout(1000)
        assert page.evaluate("window.planNode === document.querySelector('.plan-step')"), "idle plan nodes were replaced"
        page.close()


def verify_plan_handoff_browser(browser, args):
    # Two distinct registered launch definitions, represented by offline fixtures.
    # This proves host handoff boundaries, not real model interpretation.
    with session(args.urdia,args.fake_agent,["--stream"],profile=True,recommended=True) as url:
        page=browser.new_page(viewport={"width":1440,"height":1100})
        errors=[];page.on("pageerror",lambda _:errors.append(True));page.goto(url)
        expect(page.locator("#conversation-state")).to_have_text("ready",timeout=15000)
        page.locator("#project-plan > summary").click()
        expect(page.locator("#plan-context-help")).to_contain_text("new agent does not receive earlier conversation")
        objective="Agreed requirements: add_five(a,b,c,d,e); five int/float values; Python arithmetic; no extra increment; no validation or rounding; calculator.py only."
        sentinel="TRANSCRIPT_ONLY_HANDOFF_SENTINEL"
        page.locator("#plan-objective").fill(objective)
        page.locator("#plan-manual > summary").click()
        for index in range(2):
            if index:page.locator("#plan-add").click()
            row=page.locator("#plan-builder fieldset").nth(index)
            row.locator(".plan-title").fill(["Diagnose","Repair"][index])
            row.locator(".plan-scope").fill(["Read-only diagnostic. Conversation note: "+sentinel,"Edit calculator.py only. Keep test_calculator.py unchanged."][index])
            row.locator(".plan-acceptance").fill(["Report the extra increment without editing","Unchanged tests and independent expected results must pass"][index])
        page.locator("#plan-review").click()
        first=page.locator("#plan-board .plan-step").nth(0);second=page.locator("#plan-board .plan-step").nth(1)
        expect(second.get_by_role("button",name="Prepare step draft",exact=True)).to_be_disabled()
        first.get_by_role("button",name="Prepare step draft",exact=True).click()
        first_prompt=page.locator("#conversation-input").input_value()
        assert objective in first_prompt and sentinel in first_prompt
        page.locator("#conversation-submit").click()
        expect(first.locator(".plan-state")).to_have_text("Agent finished · review the result",timeout=15000)
        base=url.split("#",1)[0];headers={"Authorization":"Bearer "+url.split("#token=",1)[1]}
        def snapshot():return page.request.get(base+"api/conversation",headers=headers).json()
        assert sentinel in snapshot()["turns"][0]["response"]
        first.get_by_role("button",name="I reviewed this result").click()
        expect(second.get_by_role("button",name="Prepare step draft",exact=True)).to_be_enabled()
        page.locator("#next-profile-select").select_option("ahead");page.locator("#next-profile-apply").click()
        expect(page.locator("#next-profile-active")).to_contain_text("Recommended calculator profile")
        assert len(snapshot()["turns"])==1
        second.get_by_role("button",name="Prepare step draft",exact=True).click()
        second_prompt=page.locator("#conversation-input").input_value()
        assert objective in second_prompt and "Keep test_calculator.py unchanged" in second_prompt
        assert sentinel not in second_prompt and "Read-only diagnostic" not in second_prompt
        page.locator("#conversation-submit").click()
        expect(page.locator(".permission-card")).to_have_count(1,timeout=15000)
        page.locator(".permission-card button.reject").click()
        expect(second.locator(".plan-state")).to_have_text("Agent finished · review the result",timeout=15000)
        data=snapshot();turn=data["turns"][-1]
        assert len(data["turns"])==2 and turn["prompt"]==second_prompt
        assert sentinel not in turn["response"]
        assert turn["response"] == "fake-agent: permission rejected"
        assert turn["execution"]["provider_id"]=="selected-launch"
        assert turn["execution"]["startup_controls"]=={"model":"model-a(openai)","effort":"medium"}
        task=data["project_plan"]["plan"]["steps"][1]["task"]
        assert task["turn_id"]==turn["turn_id"] and task["profile"]["id"]=="ahead"
        assert not data["project_plan"]["plan"]["steps"][1]["reviewed"]
        assert page.request.get(base+"api/timeline",headers=headers).text().count('<details class="session"')==2
        second.get_by_role("button",name="I reviewed this result").click()
        expect(page.locator("#plan-progress")).to_contain_text("2/2 operator-reviewed")
        page.screenshot(path=str(args.output/"project-agent-handoff.png"),full_page=True)
        page.set_viewport_size({"width":390,"height":844})
        assert page.evaluate("document.documentElement.scrollWidth <= innerWidth + 1")
        page.screenshot(path=str(args.output/"project-agent-handoff-mobile.png"),full_page=True)
        assert not errors
        page.close()


def verify_project_plan_retry_browser(browser, args):
    with session(args.urdia, args.fake_agent, ["--hang", "--cancel-aware"]) as url:
        page = browser.new_page(viewport={"width":1440,"height":1100})
        page.goto(url)
        expect(page.locator("#conversation-state")).to_have_text("ready", timeout=15000)
        page.locator("#project-plan > summary").click()
        page.locator("#plan-objective").fill("One reviewed task")
        page.locator("#plan-manual > summary").click()
        page.locator(".plan-title").fill("Retry this bounded step")
        page.locator(".plan-scope").fill("Disposable project only")
        page.locator(".plan-acceptance").fill("Inspect the result manually")
        page.locator("#plan-review").click()
        page.locator("#plan-board").get_by_role("button",name="Prepare step draft").click()
        calls={"count":0}
        def lose_request(route):
            calls["count"] += 1
            if calls["count"] == 1: route.abort()
            else: route.continue_()
        page.route("**/api/conversation/prompt", lose_request)
        page.locator("#conversation-submit").click()
        expect(page.locator(".plan-state")).to_have_text("Awaiting host acknowledgement")
        expect(page.locator("#conversation-submit")).to_be_enabled()
        page.locator("#conversation-input").fill("Different unlinked draft")
        page.locator("#conversation-submit").click()
        expect(page.locator("#conversation-help")).to_contain_text("Restore the pending planned draft")
        assert calls["count"] == 1
        page.locator("#plan-board").get_by_role("button",name="Restore pending draft for retry").click()
        page.locator("#conversation-submit").click()
        expect(page.locator("#conversation-state")).to_have_text("running",timeout=15000)
        expect(page.locator("#plan-clear")).to_be_disabled()
        page.reload()
        expect(page.locator("#conversation-state")).to_have_text("running",timeout=15000)
        page.locator("#project-plan > summary").click()
        expect(page.locator("#plan-board")).to_contain_text("Attempt 1")
        expect(page.locator(".conversation-turn")).to_have_count(1)
        page.locator("#conversation-cancel").click()
        expect(page.locator(".plan-state")).to_have_text("Host task: cancelled",timeout=15000)
        assert not page.get_by_role("button",name="I reviewed this result").count()
        page.locator("#plan-board").get_by_role("button",name="Prepare retry draft").click()
        page.locator("#conversation-submit").click()
        expect(page.locator(".plan-state")).to_have_text("Agent finished · review the result",timeout=15000)
        expect(page.locator("#plan-board")).to_contain_text("Attempt 2")
        expect(page.locator(".conversation-turn")).to_have_count(2)
        assert calls["count"] == 3
        page.close()


def verify_host_plan_mutations_browser(browser, args):
    with session(args.urdia, args.fake_agent, ["--stream"]) as url:
        page = browser.new_page(viewport={"width":1440,"height":1100})
        page.goto(url)
        expect(page.locator("#conversation-state")).to_have_text("ready",timeout=15000)
        page.locator("#project-plan > summary").click()
        page.locator("#plan-objective").fill("Host-owned outline")
        page.locator("#plan-manual > summary").click()
        page.locator(".plan-title").fill("One explicit step")
        page.locator(".plan-scope").fill("Read-only")
        page.locator(".plan-acceptance").fill("Operator review")
        calls = {"count":0}
        def drop_accepted_update(route):
            calls["count"] += 1
            if calls["count"] == 1:
                route.fetch(); route.abort()
            else: route.continue_()
        page.route("**/api/conversation/plan",drop_accepted_update)
        page.locator("#plan-review").click()
        expect(page.locator("#plan-update-retry")).to_be_visible()
        page.locator("#plan-update-retry").click()
        expect(page.locator("#plan-update-retry")).to_be_hidden()
        expect(page.locator("#plan-board .plan-step")).to_have_count(1)
        assert calls["count"] == 2
        peer=browser.new_page()
        peer.goto(url)
        expect(peer.locator("#conversation-state")).to_have_text("ready",timeout=15000)
        peer.locator("#project-plan > summary").click()
        expect(peer.locator("#plan-goal")).to_have_text("Host-owned outline")
        peer.locator("#plan-board").get_by_role("button",name="Prepare step draft").click()
        page.locator("#plan-board").get_by_role("button",name="Prepare step draft").click()
        page.locator("#conversation-submit").click()
        expect(page.locator(".plan-state")).to_have_text("Agent finished · review the result",timeout=15000)
        peer.locator("#conversation-submit").click()
        expect(peer.locator("#plan-note")).to_contain_text("Plan changed",timeout=15000)
        expect(peer.locator(".conversation-turn")).to_have_count(1)
        expect(peer.locator("#plan-board")).to_contain_text("Attempt 1")
        page.locator("#plan-board").get_by_role("button",name="I reviewed this result").click()
        expect(peer.locator("#plan-progress")).to_contain_text("1/1 operator-reviewed",timeout=15000)
        peer.reload()
        expect(peer.locator("#conversation-state")).to_have_text("completed",timeout=15000)
        peer.locator("#project-plan > summary").click()
        expect(peer.locator("#plan-progress")).to_contain_text("1/1 operator-reviewed")
        assert calls["count"] == 3
        peer.close();page.close()


def verify_portable_plan_recovery(browser,args):
    with tempfile.TemporaryDirectory(prefix="urdia-plan-file-") as raw:
        saved=Path(raw)/'urdia-plan.json'
        with session(args.urdia,args.fake_agent,['--stream']) as url:
            page=browser.new_page(viewport={'width':1440,'height':1100});page.goto(url)
            expect(page.locator('#conversation-state')).to_have_text('ready',timeout=15000)
            page.locator('#project-plan > summary').click();page.locator('#plan-objective').fill('Portable calculator <img src=x onerror=alert(1)>')
            page.locator('#plan-manual > summary').click()
            for i in range(2):
                if i:page.locator('#plan-add').click()
                row=page.locator('#plan-builder fieldset').nth(i)
                row.locator('.plan-title').fill(['Arithmetic','Front end'][i]);row.locator('.plan-scope').fill('Disposable files only');row.locator('.plan-acceptance').fill('Inspect current files and checks')
            page.locator('#plan-review').click()
            first=page.locator('#plan-board .plan-step').nth(0)
            first.get_by_role('button',name='Prepare step draft').click();page.locator('#conversation-submit').click()
            expect(first.locator('.plan-state')).to_have_text('Agent finished · review the result',timeout=15000)
            first.get_by_role('button',name='I reviewed this result').click();expect(page.locator('#plan-progress')).to_contain_text('1/2 operator-reviewed')
            with page.expect_download() as download:page.locator('#plan-export').click()
            download.value.save_as(saved)
            archive=json.loads(saved.read_text());old_id=archive['source_plan_id'];old_task=archive['steps'][0]['last_task']['turn_id']
            assert archive['schema_version']==1 and archive['steps'][0]['operator_reviewed']
            assert archive['steps'][0]['last_task']['status']=='completed'
            assert 'response' not in archive['steps'][0]['last_task']
            page.close()
        # The old Urdia process is gone. A distinct host/session must review the file.
        with session(args.urdia,args.fake_agent,['--stream']) as url:
            page=browser.new_page(viewport={'width':1440,'height':1100});page.goto(url)
            expect(page.locator('#conversation-state')).to_have_text('ready',timeout=15000)
            page.locator('#project-plan > summary').click();page.locator('#plan-import summary').click()
            calls={'count':0}
            def count_previews(route):calls['count']+=1;route.continue_()
            page.route('**/api/conversation/plan/import-preview',count_previews)
            bad=Path(raw)/'invalid.json';bad.write_text('{"schema_version":2,"command":"SECRET_COMMAND"}')
            page.locator('#plan-import-file').set_input_files(bad);page.locator('#plan-import-review').click()
            expect(page.locator('#plan-note')).to_contain_text('Could not review')
            assert page.locator('.conversation-turn').count()==0 and calls['count']==1
            large=Path(raw)/'large.json';large.write_bytes(b'x'*98305)
            page.locator('#plan-import-file').set_input_files(large);page.locator('#plan-import-review').click()
            expect(page.locator('#plan-note')).to_contain_text('96 KiB limit');assert calls['count']==1
            page.locator('#plan-import-file').set_input_files(saved)
            assert calls['count']==1
            page.locator('#plan-import-review').click()
            expect(page.locator('#plan-import-preview')).to_be_visible()
            expect(page.locator('#plan-import-preview')).to_contain_text('Imported file claims only')
            assert not page.locator('#project-plan img').count()
            expect(page.locator('#plan-import-start')).to_have_value('0')
            expect(page.locator('#plan-import-restore')).to_be_disabled()
            page.locator('#plan-import-confirm').check();page.locator('#plan-import-start').select_option('1')
            expect(page.locator('#plan-import-confirm')).not_to_be_checked()
            expect(page.locator('#plan-import-restore')).to_be_disabled()
            page.locator('#plan-import-confirm').check()
            dropped={'once':False}
            def lose_restore_response(route):
                if not dropped['once']:dropped['once']=True;route.fetch();route.abort()
                else:route.continue_()
            page.route('**/api/conversation/plan',lose_restore_response)
            page.locator('#plan-import-restore').click()
            expect(page.locator('#plan-update-retry')).to_be_visible(timeout=15000)
            page.locator('#plan-update-retry').click();expect(page.locator('#plan-update-retry')).to_be_hidden()
            expect(page.locator('#plan-board .plan-step')).to_have_count(1)
            expect(page.locator('.plan-state')).to_have_text('Ready to prepare')
            expect(page.locator('#plan-progress')).to_contain_text('0/1 operator-reviewed')
            expect(page.locator('#summary-completed')).to_have_text('0/1')
            expect(page.locator('#summary-reviewed')).to_have_text('0/1')
            expect(page.locator('#summary-checked')).to_have_text('0/1')
            expect(page.locator('#project-summary-note')).to_contain_text('saved-history steps are separate')
            assert page.locator('.conversation-turn').count()==0
            base=url.split('#',1)[0];headers={'Authorization':'Bearer '+url.split('#token=',1)[1]}
            snapshot=page.request.get(base+'api/conversation',headers=headers).json();new_id=snapshot['project_plan']['plan']['id']
            assert new_id!=old_id and snapshot['project_plan']['revision']==1
            assert snapshot['project_plan']['plan']['steps'][0]['task'] is None
            rejected=page.request.post(base+'api/conversation/prompt',headers=headers,data={'turn_id':str(uuid.uuid4()),'text':'Old saved prompt','plan_link':{'plan_id':old_id,'revision':1,'step_index':0}})
            assert rejected.status==409
            page.locator('#plan-recovery summary').click();expect(page.locator('#plan-recovery-history')).to_contain_text(old_task[:8])
            expect(page.locator('#plan-recovery-history')).to_contain_text('Earlier stages omitted after your review: 1')
            page.screenshot(path=str(args.output/'project-plan-recovery.png'),full_page=True)
            page.set_viewport_size({'width':390,'height':844});assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
            page.screenshot(path=str(args.output/'project-plan-recovery-mobile.png'),full_page=True)
            page.reload();expect(page.locator('#conversation-state')).to_have_text('ready',timeout=15000)
            page.locator('#project-plan > summary').click();expect(page.locator('.plan-state')).to_have_text('Ready to prepare')
            expect(page.locator('#plan-recovery')).to_be_visible()
            page.locator('#plan-board').get_by_role('button',name='Prepare step draft').click()
            expect(page.locator('#conversation-input')).to_have_value(re.compile('Step 1/1: Front end'))
            page.locator('#conversation-submit').click()
            expect(page.locator('.plan-state')).to_have_text('Agent finished · review the result',timeout=15000)
            expect(page.locator('.conversation-turn')).to_have_count(1)
            with page.expect_download() as download:page.locator('#plan-export').click()
            again=Path(raw)/'again.json';download.value.save_as(again);continued=json.loads(again.read_text())
            assert len(continued['steps'])==2 and continued['steps'][0]['inherited_history']
            assert continued['steps'][0]['last_task']['turn_id']==old_task
            assert continued['steps'][1]['last_task']['turn_id']!=old_task
            assert continued['steps'][1]['last_task']['status']=='completed'
            page.close()


def run(args):
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    with sync_playwright() as driver:
        browser = driver.chromium.launch(executable_path=str(args.chromium), args=["--no-sandbox"])
        try:
            verify_python_project_browser(browser,args)
            verify_project_summary(browser,args)
            verify_agent_reconnection(browser,args)
            verify_provider_failure_browser(browser,args)
            verify_plan_proposal_browser(browser, args)
            verify_project_plan_browser(browser, args)
            verify_plan_handoff_browser(browser, args)
            verify_project_plan_retry_browser(browser, args)
            verify_host_plan_mutations_browser(browser, args)
            check_step_advice_execution(browser, args)
            verify_portable_plan_recovery(browser,args)
            verify_calculator_browser(browser, args)
            check_profile_switching(browser,args)
            check_setup_guide(browser,args)
            check_agent_registration(browser,args)
            check_profile_editor(browser,args)
            check_jev_settings_browser(browser, args)
            check_laya_browser(browser, args)
            check_recommended_execution(browser, args)
            check_automatic_jev_browser(browser, args)
            with session(args.urdia, args.fake_agent, ["--stream"]) as url:
                page = browser.new_page(viewport={"width": 1440, "height": 1000})
                errors = []
                page.on("pageerror", lambda error: errors.append(str(error)))
                page.goto(url)
                expect(page.locator("#project-name")).to_have_text("Example project")
                expect(page.locator("#agent-name")).to_have_text("Offline reference agent (not AI)")
                assert "#token" not in page.url
                page.screenshot(path=str(args.output / "workspace-ready.png"), full_page=True)
                page.locator('[data-prompt]').first.click()
                expect(page.locator("#conversation-input")).to_have_value(re.compile("Review this project"))
                # Literal HTML must remain inert even when echoed by the agent.
                prompt = '<img src=x onerror="window.injected=true"> Explain the next step.'
                page.locator("#conversation-input").fill(prompt)
                page.locator("#conversation-submit").click()
                expect(page.locator("#task-title")).to_have_text("Task completed", timeout=15000)
                expect(page.locator(".conversation-response")).to_contain_text(prompt)
                assert page.locator("#conversation-history img").count() == 0
                assert page.evaluate("window.injected === undefined")
                page.locator("#conversation-input").fill("Summarize the result.")
                page.locator("#conversation-submit").click()
                expect(page.locator(".conversation-turn")).to_have_count(2)
                expect(page.locator("#task-title")).to_have_text("Task completed", timeout=15000)
                page.locator("#outcome-technical > summary").click()
                expect(page.locator("#outcome-technical #task-metrics")).to_be_visible()
                assert page.locator("#task-metrics > div").first.evaluate("n=>getComputedStyle(n).display") == "block", "metric cards lost vertical layout"
                expect(page.locator("#metric-updates")).to_have_text(re.compile(r"[1-9][0-9]*"))
                expect(page.locator("#metric-first-text")).to_have_text(re.compile(r"(?:[0-9]+ ms|[0-9]+\.[0-9] s)"))
                expect(page.locator("#metric-chars")).to_have_text(str(len("fake-agent: Summarize the result.")))
                frozen_elapsed = page.locator("#metric-elapsed").inner_text()
                page.wait_for_timeout(1100)
                expect(page.locator("#metric-elapsed")).to_have_text(frozen_elapsed)
                # Unchanged polling must not replace a reader's transcript DOM.
                page.locator(".conversation-turn").first.evaluate("node => node.dataset.sentinel = 'kept'")
                page.wait_for_timeout(1100)
                assert page.locator(".conversation-turn").first.get_attribute("data-sentinel") == "kept"
                page.screenshot(path=str(args.output / "workspace-completed.png"), full_page=True)
                page.locator('.navigation [data-view="activity"]').click()
                expect(page.locator("#activity-view")).to_be_visible()
                expect(page.frame_locator("#timeline").get_by_text("Session not closed", exact=True)).to_be_visible()
                page.locator('.navigation [data-view="settings"]').click()
                expect(page.locator("#settings-view")).to_be_visible()
                assert page.locator('input[type="password"]').count() == 2
                expect(page.locator("#laya-form")).to_be_visible()
                page.screenshot(path=str(args.output / "decision-engines.png"), full_page=True)
                page.locator('.navigation [data-view="workspace"]').click()
                for width in [1280, 820, 390]:
                    page.set_viewport_size({"width": width, "height": 844})
                    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth"), "horizontal overflow"
                    expect(page.locator("#conversation-submit")).to_be_visible()
                page.screenshot(path=str(args.output / "workspace-mobile.png"), full_page=True)
                assert not errors, "browser JavaScript error (private details omitted)"
                page.close()
                # Controlled HTTP fixtures exercise failure and history-only shells;
                # no real provider failure or credential diagnosis is implied.
                history = browser.new_page()
                history.route("**/api/conversation", lambda route: route.fulfill(status=404, body=""))
                history.goto(url)
                expect(history.locator("#activity-view")).to_be_visible()
                expect(history.locator("#conversation")).to_be_hidden()
                history.close()
                failed = browser.new_page()
                failed.route("**/api/conversation", lambda route: route.fulfill(json={
                    "sequence": 1, "active_turn_id": None, "turns": [{
                        "turn_id": "failed-fixture", "status": "failed", "prompt": "Example failed task",
                        "response": "", "error": "The provider turn failed."
                    }]}))
                failed.goto(url)
                expect(failed.locator("#task-title")).to_have_text("Task failed")
                expect(failed.locator("#conversation-submit")).to_be_enabled()
                failed.close()
                locked = browser.new_page()
                locked.goto(url.split("#")[0])
                expect(locked.locator("#locked")).to_be_visible()
                expect(locked.locator("#workspace-view")).to_be_hidden()
                locked.close()
            with session(args.urdia, args.fake_agent, ["--stream"], advice=True) as url:
                page = browser.new_page(viewport={"width": 1440, "height": 1000})
                page.goto(url)
                page.locator('.navigation [data-view="settings"]').click()
                expect(page.locator("#advisory-form")).to_be_visible()
                expect(page.locator("#advisory-engine")).to_have_value("deterministic")
                page.locator("#advisory-objective").fill("Review the project and propose focused tests.")
                page.locator("#advisory-submit").click()
                expect(page.locator("#advisory-result")).to_contain_text("Recommended agent: offline", timeout=15000)
                expect(page.locator("#advisory-status")).to_contain_text("Host decision time:")
                expect(page.locator("#advisory-result")).to_contain_text("Task verification: not performed")
                expect(page.locator("#agent-name")).to_have_text("Offline reference agent (not AI)")
                # Advice is not a conversation turn and never executes the agent.
                assert page.locator(".conversation-turn").count() == 0
                page.screenshot(path=str(args.output / "workspace-advisory.png"), full_page=True)
                page.set_viewport_size({"width": 390, "height": 844})
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                page.close()
                # Remote consent and review/error rendering use controlled fixtures;
                # no hosted API key, remote model or charge is involved.
                remote = browser.new_page(viewport={"width": 1440, "height": 1000})
                received = []
                latest = [None]
                def remote_fixture(route):
                    if route.request.method == "GET":
                        route.fulfill(json={"engines": [{"id": "jev", "configured": True, "remote": True}], "latest": latest[0]})
                    else:
                        body = route.request.post_data_json
                        received.append(body)
                        result = {"selected_provider": None, "probabilities": {"qwen": 0.2},
                                  "review_probability": 0.8, "confidence": 0.61,
                                  "probability_semantics": "Calibrated", "fallback_reason": "no_selection"}
                        if len(received) == 2:
                            result.update(probabilities={}, review_probability=None, confidence=None,
                                          probability_semantics="Unknown", fallback_reason="provider_error")
                        latest[0] = {"request_id": body["request_id"], "engine": "jev", "status": "completed", "elapsed_ms": 142, "result": result}
                        route.fulfill(status=202, json=latest[0])
                remote.route("**/api/advisory", remote_fixture)
                remote.goto(url)
                remote.locator('.navigation [data-view="settings"]').click()
                expect(remote.locator("#advisory-form")).to_be_visible()
                remote.locator("#advisory-objective").fill("Review a Python project and add tests.")
                remote.locator("#advisory-submit").click()
                assert not received, "remote request must require explicit consent"
                remote.locator("#advisory-consent").check()
                remote.locator("#advisory-submit").click()
                expect(remote.locator("#advisory-result")).to_contain_text("Review needed", timeout=15000)
                assert received[0]["allow_remote"] is True
                expect(remote.locator("#advisory-result")).to_contain_text("80.0%")
                remote.screenshot(path=str(args.output / "workspace-advisory-review.png"), full_page=True)
                expect(remote.locator("#advisory-consent")).not_to_be_checked()
                remote.locator("#advisory-consent").check()
                remote.locator("#advisory-submit").click()
                expect(remote.locator("#advisory-result")).to_contain_text("The provider request failed", timeout=15000)
                assert len(received) == 2
                expect(remote.locator("#advisory-status")).to_contain_text("Host decision time: 142 ms")
                latest[0] = {"request_id": received[-1]["request_id"], "engine": "jev", "status": "failed", "elapsed_ms": 35000, "result": None}
                expect(remote.locator("#advisory-status")).to_contain_text("Host decision time: 35000 ms", timeout=15000)
                expect(remote.locator("#advisory-result")).to_be_hidden()
                remote.close()
            with session(args.urdia, args.fake_agent, ["--hang", "--cancel-aware"]) as url:
                page = browser.new_page()
                page.goto(url)
                expect(page.locator("#conversation")).to_be_visible()
                page.locator("#conversation-input").fill("Wait until cancelled.")
                page.locator("#conversation-submit").click()
                expect(page.locator("#conversation-state")).to_have_text("running", timeout=15000)
                page.locator("#conversation-input").fill("Draft for the next task")
                page.locator("#conversation-cancel").click()
                expect(page.locator("#task-title")).to_have_text("Task stopped", timeout=15000)
                expect(page.locator("#conversation-input")).to_have_value("Draft for the next task")
                page.close()
            with session(args.urdia, args.fake_agent, ["--request-permission"]) as url:
                page = browser.new_page()
                page.goto(url)
                expect(page.locator("#conversation")).to_be_visible()
                page.locator("#conversation-input").fill("Ask for a one-shot permission.")
                page.locator("#conversation-submit").click()
                expect(page.locator(".permission-card")).to_have_count(1, timeout=15000)
                expect(page.locator("#task-title")).to_have_text("Your decision is needed", timeout=15000)
                page.screenshot(path=str(args.output / "workspace-permission.png"), full_page=True)
                page.locator(".permission-card button.reject").click()
                expect(page.locator("#task-title")).to_have_text("Task completed", timeout=15000)
                expect(page.locator(".conversation-response")).to_contain_text("permission rejected")
                page.locator("#conversation-input").fill("Ask again for a one-shot permission.")
                page.locator("#conversation-submit").click()
                expect(page.locator(".permission-card")).to_have_count(1, timeout=15000)
                page.locator(".permission-card button.allow").click()
                # Execution requests require the existing explicit second click.
                expect(page.locator(".permission-card button.allow")).to_contain_text("Confirm")
                page.locator(".permission-card button.allow").click()
                expect(page.locator("#task-title")).to_have_text("Task completed", timeout=15000)
                expect(page.locator(".conversation-response").last).to_contain_text("Ask again")
                page.close()
            with session(args.urdia, args.fake_agent, ["--request-permission"], trusted=True) as url:
                page = browser.new_page()
                page.goto(url)
                expect(page.locator("#conversation")).to_be_visible()
                page.locator("#conversation-input").fill("Prepare explicitly scoped trusted access.")
                page.locator("#conversation-submit").click()
                expect(page.locator("#trusted-prepare")).to_be_enabled(timeout=15000)
                page.locator("#trusted-prepare").click()
                expect(page.locator("#trusted-confirm")).to_be_visible()
                expect(page.locator("#trusted-status")).to_have_text("awaiting confirmation")
                page.locator("#trusted-confirm").click()
                expect(page.locator("#trusted-status")).to_have_text("active", timeout=15000)
                page.locator("#trusted-revoke").click()
                expect(page.locator("#trusted-status")).to_have_text("revoked", timeout=15000)
                page.close()
            with session(args.urdia, args.fake_agent, ["--stream"], profile=True, advice=True, profile_advice=True) as url:
                page = browser.new_page(viewport={"width": 1440, "height": 1000})
                page.goto(url)
                expect(page.locator("#execution-profile")).to_be_visible()
                expect(page.locator("#profile-name")).to_have_text("Offline backend profile")
                expect(page.locator("#profile-selection")).to_have_text("Your default")
                expect(page.locator("#profile-model")).to_have_text("<img src=x onerror=alert(1)>")
                expect(page.locator("#profile-model img")).to_have_count(0)
                expect(page.locator("#profile-effort")).to_have_text("Not configured")
                expect(page.locator("#startup-controls")).to_contain_text("model model-b(openai) · effort none")
                expect(page.locator("#startup-controls")).to_contain_text("Remote inference is not verified")
                page.screenshot(path=str(args.output / "workspace-profile.png"), full_page=True)
                page.set_viewport_size({"width": 390, "height": 844})
                expect(page.locator("#profile-name")).to_be_visible()
                assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth"), "profile layout overflows"
                page.screenshot(path=str(args.output / "workspace-profile-mobile.png"), full_page=True)
                page.set_viewport_size({"width": 1440, "height": 1000})
                page.locator('.navigation [data-view="settings"]').click()
                expect(page.locator("#advisory-title")).to_have_text("Find a profile for your task")
                page.locator("#advisory-engine").select_option("deterministic")
                page.locator("#advisory-objective").fill("Build a small calculator")
                page.locator("#advisory-submit").click()
                expect(page.locator("#advisory-result")).to_contain_text("Recommended profile: Offline backend profile", timeout=15000)
                expect(page.locator("#advisory-result img")).to_have_count(0)
                expect(page.locator("#advisory-result")).to_contain_text("active profile is unchanged")
                page.screenshot(path=str(args.output / "workspace-profile-advice.png"), full_page=True)
                page.locator('.navigation [data-view="workspace"]').click()
                expect(page.locator("#profile-selection")).to_have_text("Your default")
                expect(page.locator("#conversation-history")).to_be_empty()
                page.close()
            with session(args.urdia, args.fake_agent, ["--controls-drift"], profile=True) as url:
                page = browser.new_page(viewport={"width": 1440, "height": 1000})
                page.goto(url)
                expect(page.locator("#startup-controls")).to_contain_text("Latest agent report (#0)")
                page.locator("#conversation-input").fill("First offline task")
                page.locator("#conversation-submit").click()
                expect(page.locator("#task-title")).to_have_text("Task completed", timeout=15000)
                expect(page.locator("#startup-controls")).to_contain_text("Settings changed since startup", timeout=15000)
                expect(page.locator("#startup-controls")).to_contain_text("another task is blocked")
                page.locator("#conversation-input").fill("Blocked offline task")
                page.locator("#conversation-submit").click()
                expect(page.locator("#task-title")).to_have_text("Task failed", timeout=15000)
                page.screenshot(path=str(args.output / "workspace-control-drift.png"), full_page=True)
                page.set_viewport_size({"width": 390, "height": 844})
                assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth"), "drift layout overflows"
                page.close()
            with session(args.urdia, args.fake_agent, ["--tool-activity", "--stream"]) as url:
                page = browser.new_page(viewport={"width": 1440, "height": 1100})
                page.goto(url)
                page.locator("#conversation-input").fill("Inspect the project, make a small edit and run its tests.")
                page.locator("#conversation-submit").click()
                expect(page.locator("#task-title")).to_have_text("Task completed", timeout=15000)
                expect(page.locator("#tool-summary")).to_have_text("3 observed · 2 completed · 1 failed")
                expect(page.locator("#tool-calls li")).to_have_count(3)
                expect(page.locator("#tool-calls")).to_contain_text("#1 Read")
                expect(page.locator("#tool-calls")).to_contain_text("#2 Edit")
                expect(page.locator("#tool-calls")).to_contain_text("#3 Execute")
                token = url.split("#token=", 1)[1]
                response = page.request.get(url.split("#", 1)[0] + "api/conversation", headers={"Authorization": "Bearer " + token})
                assert response.ok
                assert "TOOL_PRIVATE" not in response.text(), "raw tool metadata leaked into conversation API"
                assert "TOOL_PRIVATE" not in page.content(), "raw tool metadata leaked into DOM"
                page.screenshot(path=str(args.output / "workspace-tool-activity.png"), full_page=True)
                page.set_viewport_size({"width": 390, "height": 844})
                assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth"), "tool activity layout overflows"
                page.screenshot(path=str(args.output / "workspace-tool-activity-mobile.png"), full_page=True)
                page.close()
        finally:
            browser.close()
    print("PASS: generic stdlib Python project checks with actual pass, empty/failing suite refusal, explicit consent, cancellation and desktop/mobile; read-only per-step project summary with distinct completion/review/check counts, actual task checks, historical evidence, literal text, task navigation and mobile layout; explicit original-agent recovery with failed/successful startup, stable refusal, renewed consent, preserved linked plan/draft, no task replay, stale refusal and reviewed-context stage retry; fixed provider failure messages in sidebar/transcript/history, private native fields omitted, no automatic retry and explicit second task, five categories and desktop/mobile; explicit reviewed context handoff across fresh configured profiles, current objective/scope/checks, omitted prior-step transcript note, exact selector and stage binding with one-shot denial; objective-first planning-message preparation, inert strict proposal preview, preserved drafts on failure, literal rendering, explicit editable adoption and plan acceptance without another task, stale source refusal and collapsed-field native validation; inert operator project plans, host UUID binding after lost admission response, idempotent retries, explicit review before next-step preparation, draft preservation, eight-step bounds, host-session plan restoration after completed/active refresh and unreviewed draft reset; explicitly linked per-step Laya profile advice, inert preparation, exact selectors, lost-confirmation replay, one-shot rejection, restored stage review and stale-ticket refusal; explicit private plan downloads, strict bounded import preview, distinct-host recovery with renewed consent and fresh UUID/task state, stale-link refusal, inherited history and continuation export; real bundled two-/five-number calculator verification success/independent failure/cancellation, inert matching draft and renewed consent after contract change, explicit consent, busy task exclusion, historical stale result and mobile layout; manual next-profile switching without restart, explicit inert selection, stale/busy/auth rejection, exact selectors in two distinct fresh sessions, one-shot denial, original-session restoration and disabled trusted-mode switching; guided setup with inert navigation, shared form drafts, pending-restart block, explicit single test, stable-ID uncertain-delivery retry, honest response review and measured time; explicit prompt-free selector inspection and model-conditioned effort options applied after restart; single-restart staged agent/profile onboarding, redacted catalog, draft preservation, restart/profile/model application; private profile editor save/restart boundary and draft preservation; explicit profile review/confirm via local Laya fixture, different dedicated launch with exact controls, one-shot denial and unchanged next default task; local Laya connect/test/advice/reconnect failure/disconnect without CLI setup or credential retention; offline browser navigation, two turns, inert HTML, stable transcript, frozen host task metrics, redacted tool activity, responsive layout, auth lock, cancellation draft, one-shot reject/confirm, trusted prepare/confirm/revoke, advisory routing without execution, failure/history fixtures, default profile metadata with inert labels, and live configuration drift with a blocked next task at desktop/mobile sizes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--urdia", type=Path, required=True)
    parser.add_argument("--fake-agent", type=Path, required=True)
    parser.add_argument("--chromium", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    for name in ["urdia", "fake_agent", "chromium", "output"]:
        setattr(options, name, getattr(options, name).resolve())
    run(options)
