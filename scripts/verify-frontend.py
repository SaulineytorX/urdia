#!/usr/bin/env python3
"""Bounded static-page browser checks; explicit execution consent, no model calls."""
import argparse
import hashlib
import json
import mimetypes
import os
import secrets
from pathlib import Path
import stat
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote, urlsplit

EXTENSIONS={'.html','.css','.js','.json','.svg','.png','.jpg','.jpeg','.ico','.woff','.woff2'}
EXCLUDED={'.git','.urdia','node_modules','vendor','venv','__pycache__'}

def snapshot(root):
    files={};total=0
    for directory,dirs,names in os.walk(root,followlinks=False):
        parent=Path(directory)
        if len(parent.relative_to(root).parts)>8:raise ValueError('source depth')
        dirs[:]=[name for name in dirs if not name.startswith('.') and name not in EXCLUDED]
        if any((parent/name).is_symlink() for name in dirs):raise ValueError('redirected directory')
        for name in sorted(names):
            path=parent/name
            if name.startswith('.') or path.suffix.lower() not in EXTENSIONS:continue
            fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK)
            with os.fdopen(fd,'rb') as stream:
                info=os.fstat(stream.fileno())
                if not stat.S_ISREG(info.st_mode) or info.st_nlink!=1:raise ValueError('nonregular source')
                data=stream.read(1048577)
            if len(data)>1048576:raise ValueError('source size')
            total+=len(data);files[path.relative_to(root).as_posix()]=data
            if len(files)>128 or total>8388608:raise ValueError('source bound')
    if 'index.html' not in files:raise ValueError('missing index')
    return files

def fingerprint(files):
    h=hashlib.sha256()
    for name,data in sorted(files.items()):h.update(name.encode()+b'\0'+hashlib.sha256(data).digest())
    return h.hexdigest()

def verify(root,contract,browser):
    from playwright.sync_api import sync_playwright, TimeoutError as BrowserTimeout
    files=snapshot(root);before=fingerprint(files);token=secrets.token_urlsafe(32)
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            if not secrets.compare_digest(self.headers.get('X-Urdia-Frontend-Token',''),token):self.send_error(404);return
            path=unquote(urlsplit(self.path).path).lstrip('/') or 'index.html'
            if path not in files:self.send_error(404);return
            self.send_response(200)
            self.send_header('Content-Type',mimetypes.guess_type(path)[0] or 'application/octet-stream')
            self.send_header('Content-Security-Policy',"default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; font-src 'self'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'")
            self.send_header('X-Content-Type-Options','nosniff');self.end_headers();self.wfile.write(files[path])
        def log_message(self,*args):pass
    server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    origin='http://127.0.0.1:'+str(server.server_port);checks={};counts={'viewports':2,'checks_run':0,'errors':0,'blocked_requests':0}
    try:
        with sync_playwright() as driver:
            app=driver.chromium.launch(executable_path=browser,chromium_sandbox=True,args=['--disable-background-networking','--disable-sync','--no-first-run'])
            try:
                for label,size in [('desktop',{'width':1280,'height':800}),('mobile',{'width':390,'height':844})]:
                    started=time.monotonic();errors=[];context=app.new_context(viewport=size,is_mobile=(label=='mobile'),has_touch=(label=='mobile'),service_workers='block',accept_downloads=False,extra_http_headers={'X-Urdia-Frontend-Token':token})
                    page=context.new_page();page.set_default_timeout(3000)
                    page.on('pageerror',lambda error:errors.append(True) if len(errors)<1000 else None)
                    page.on('requestfailed',lambda request:errors.append(True) if len(errors)<1000 else None)
                    page.on('response',lambda response:errors.append(True) if response.status>=400 and len(errors)<1000 else None)
                    page.add_init_script("window.addEventListener('securitypolicyviolation',()=>{ window.__urdiaPolicyViolation=true; });")
                    def route(request):
                        url=urlsplit(request.request.url)
                        if url.scheme=='http' and url.netloc==urlsplit(origin).netloc and request.request.method=='GET' and (unquote(url.path).lstrip('/') or 'index.html') in files:request.continue_()
                        else:counts['blocked_requests']=min(10000,counts['blocked_requests']+1);request.abort()
                    context.route('**/*',route)
                    passed=False;reason='load'
                    try:
                        response=page.goto(origin+'/',wait_until='load',timeout=5000)
                        assert response and response.status==200
                        page.wait_for_timeout(250)
                        reason='layout'
                        assert abs(page.evaluate('window.innerWidth')-size['width'])<=1
                        assert page.evaluate('document.documentElement.scrollWidth<=window.innerWidth')
                        counts['checks_run']+=2
                        if contract=='frontend-counter-v1':
                            reason='counter'
                            assert page.locator('#value').inner_text().strip()=='0'
                            for _ in range(3):page.locator('button#increment').click()
                            assert page.locator('#value').inner_text().strip()=='3'
                            for _ in range(4):
                                if page.locator('button#decrement').is_enabled():page.locator('button#decrement').click()
                                else:assert page.locator('#value').inner_text().strip()=='0'
                            assert page.locator('#value').inner_text().strip()=='0'
                            page.locator('button#increment').focus();page.keyboard.press('Enter')
                            assert page.locator('#value').inner_text().strip()=='1'
                            page.locator('button#reset').click();assert page.locator('#value').inner_text().strip()=='0'
                            counts['checks_run']+=5
                        page.wait_for_timeout(100)
                        reason='runtime'
                        assert not errors and not page.evaluate('Boolean(window.__urdiaPolicyViolation)')
                        passed=True
                    except BrowserTimeout:counts['errors']+=1;reason='timeout'
                    except Exception:counts['errors']+=1
                    finally:
                        counts['errors']+=len(errors);context.close()
                    checks[label]={'failure_reason':None if passed else reason,'status':'passed' if passed else 'failed','exit_code':0 if passed else 1,'elapsed_ms':round((time.monotonic()-started)*1000)}
            finally:app.close()
    finally:server.shutdown();server.server_close()
    unchanged=before==fingerprint(snapshot(root))
    passed=unchanged and counts['errors']==0 and counts['blocked_requests']==0 and all(c['status']=='passed' for c in checks.values())
    return {'schema_version':4,'contract':contract,'source_file_count':len(files),'status':'passed' if passed else 'failed','files_sha256_before':{'frontend_sources':before},'acceptance_files_unchanged':unchanged,'frontend':counts,'checks':checks}

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workspace',required=True);parser.add_argument('--contract',choices=['frontend-static-v1','frontend-counter-v1'],required=True)
    parser.add_argument('--browser',required=True);parser.add_argument('--confirm-execution',action='store_true');args=parser.parse_args()
    if not args.confirm_execution:return 2
    try:report=verify(Path(args.workspace).resolve(strict=True),args.contract,args.browser)
    except Exception:
        print('Frontend check unavailable: review runtime, sandbox and static source requirements.',file=sys.stderr);return 2
    print(json.dumps(report));return 0 if report['status']=='passed' else 1

if __name__=='__main__':raise SystemExit(main())
