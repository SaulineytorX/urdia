"""Independent browser acceptance for the fixed Launchboard contract."""
import argparse,contextlib,hashlib,json,threading
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from playwright.sync_api import sync_playwright,expect


def check(workspace,output):
 workspace=workspace.resolve(strict=True);output.mkdir(parents=True,exist_ok=True)
 files=['index.html','styles.css','app.js'];before={n:hashlib.sha256((workspace/n).read_bytes()).hexdigest() for n in files}
 class Handler(BaseHTTPRequestHandler):
  def do_GET(self):
   name={'/':'index.html','/index.html':'index.html','/styles.css':'styles.css','/app.js':'app.js'}.get(self.path)
   if not name:self.send_error(404);return
   self.send_response(200);self.send_header('Content-Type',{'index.html':'text/html; charset=utf-8','styles.css':'text/css','app.js':'application/javascript'}[name]);self.send_header('Content-Security-Policy',"default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'");self.end_headers();self.wfile.write((workspace/name).read_bytes())
  def log_message(self,*a):pass
 server=ThreadingHTTPServer(('127.0.0.1',0),Handler);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start();origin='http://127.0.0.1:'+str(server.server_port)
 results=[];errors=[];external=[]
 def record(name,fn):
  try:fn();results.append({'check':name,'passed':True})
  except Exception as e:results.append({'check':name,'passed':False,'error_kind':type(e).__name__});raise
 try:
  with sync_playwright() as driver:
   browser=driver.chromium.launch(executable_path='/opt/google/chrome/chrome',chromium_sandbox=True)
   try:
    context=browser.new_context(viewport={'width':1440,'height':1000},record_video_dir=str(output/'video'),record_video_size={'width':1440,'height':1000})
    def route(r):
     if r.request.url in {origin+'/',origin+'/index.html',origin+'/styles.css',origin+'/app.js'}:r.continue_()
     else:external.append(r.request.url);r.abort()
    context.route('**/*',route)
    page=context.new_page();page.on('pageerror',lambda e:errors.append(type(e).__name__));page.goto(origin+'/');page.evaluate("localStorage.setItem('urdia-other-app-sentinel','keep')")
    def eq(selector,value):expect(page.locator(selector)).to_have_text(value)
    def reset(accept=True):
     page.once('dialog',lambda d:d.accept() if accept else d.dismiss());page.locator('#reset-board').click()
    def seed():
     expect(page.locator('.task-card')).to_have_count(6);eq('#total-count','6');eq('#done-count','1');eq('#completion-rate','17%');expect(page.locator('[data-lane="backlog"] [data-action="back"]:visible:enabled')).to_have_count(0);expect(page.locator('[data-lane="done"] [data-action="next"]:visible:enabled')).to_have_count(0)
    record('seed_and_global_metrics',seed)
    page.screenshot(path=str(output/'desktop.png'),full_page=True)
    def filters():
     page.locator('#search').fill('user journey');expect(page.locator('.task-card:visible')).to_have_count(1)
     page.locator('#priority-filter').select_option('low');expect(page.locator('.task-card:visible')).to_have_count(0)
     eq('#total-count','6');eq('#done-count','1');page.locator('#search').fill('');page.locator('#priority-filter').select_option('all')
    record('combined_filters_preserve_global_metrics',filters)
    def filtered_mutation():
     page.locator('#search').fill('user journey');page.locator('[data-task-id="discover"] [data-action="next"]').click();page.locator('#search').fill('');page.reload();expect(page.locator('.task-card')).to_have_count(6);reset();seed()
    record('filtered_mutation_preserves_hidden_tasks',filtered_mutation)
    def moves():
     task=page.locator('[data-task-id="discover"]');task.locator('[data-action="next"]').click();expect(page.locator('[data-lane="progress"] [data-task-id="discover"]')).to_have_count(1)
     task=page.locator('[data-task-id="discover"]');task.locator('[data-action="next"]').click();eq('#done-count','2');eq('#completion-rate','33%')
     page.locator('[data-task-id="discover"] [data-action="back"]').click();eq('#done-count','1');page.reload();expect(page.locator('[data-lane="progress"] [data-task-id="discover"]')).to_have_count(1)
    record('move_forward_back_and_reload_persistence',moves)
    def keyboard_focus():
     page.locator('[data-task-id="api"] [data-action="next"]').focus();page.keyboard.press('Enter');expect(page.locator('[data-lane="progress"] [data-task-id="api"]')).to_have_count(1)
     assert page.evaluate('document.activeElement!==document.body && document.activeElement.tagName==="BUTTON"')
    record('keyboard_move_retains_usable_focus',keyboard_focus)
    def dialog_cancel():
     page.locator('#new-task').click();expect(page.locator('#task-dialog')).to_be_visible();page.locator('#task-title').fill('Cancelled');page.locator('#cancel-task').click();expect(page.locator('.task-card')).to_have_count(6)
     page.locator('#new-task').click();expect(page.locator('#task-title')).to_have_value('');page.keyboard.press('Escape');expect(page.locator('#task-dialog')).not_to_be_visible()
    record('cancel_escape_and_clean_dialog',dialog_cancel)
    title='<img src=x onerror=alert(1)>'
    def create():
     page.locator('#new-task').click();page.locator('#task-title').fill('   ');page.locator('#save-task').click();expect(page.locator('#task-dialog')).to_be_visible();expect(page.locator('.task-card')).to_have_count(6);expect(page.locator('#task-dialog [role="alert"]:visible')).not_to_have_count(0)
     page.locator('#task-title').fill(title);page.locator('#task-description').fill('A real safely rendered task.');page.locator('#task-priority').select_option('high');page.locator('#save-task').click();expect(page.locator('#task-dialog')).not_to_be_visible();expect(page.locator('.task-card')).to_have_count(7);expect(page.locator('#new-task')).to_be_focused();eq('#total-count','7')
     expect(page.get_by_text(title,exact=True)).to_be_visible();expect(page.locator('.task-card img')).to_have_count(0);page.reload();expect(page.get_by_text(title,exact=True)).to_be_visible()
    record('validation_safe_text_create_focus_and_reload',create)
    def export():
     with page.expect_download() as download:page.locator('#export-board').click()
     data=json.loads(Path(download.value.path()).read_text());assert isinstance(data,list) and len(data)==7 and any(t['title']==title for t in data)
    record('json_export_contains_actual_tasks',export)
    def delete():
     card=page.locator('.task-card').filter(has=page.get_by_text(title,exact=True));card.locator('[data-action="delete"]').click();expect(page.locator('.task-card')).to_have_count(6);eq('#total-count','6');page.reload();expect(page.locator('.task-card')).to_have_count(6);expect(page.get_by_text(title,exact=True)).to_have_count(0)
    record('delete_updates_actual_counts',delete)
    def reset_confirm():
     reset(False);expect(page.locator('[data-lane="progress"] [data-task-id="api"]')).to_have_count(1)
     page.locator('#search').fill('impossible-term');page.locator('#priority-filter').select_option('low');reset();seed();expect(page.locator('#search')).to_have_value('');expect(page.locator('#priority-filter')).to_have_value('all');assert page.evaluate("localStorage.getItem('urdia-other-app-sentinel')")=='keep'
    record('reset_cancel_confirm_and_filters',reset_confirm)
    def mobile():
     for label,width in [('mobile',390),('narrow',320)]:
      page.set_viewport_size({'width':width,'height':900});assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
      expect(page.locator('#new-task')).to_be_visible();page.locator('#new-task').click();expect(page.locator('#task-title')).to_be_visible();page.locator('#cancel-task').click();page.screenshot(path=str(output/(label+'.png')),full_page=True)
    record('mobile_390_and_320_no_overflow_usable_dialog',mobile)
    def empty():
     page.evaluate("localStorage.setItem('urdia-launchboard-v1','[]')");page.reload();expect(page.locator('.task-card')).to_have_count(0);eq('#total-count','0');eq('#completion-rate','0%')
    record('empty_saved_board_is_not_reseeded',empty)
    def malformed():
     invalid=['{','{}','[{"id":"a","title":"X","description":"","priority":"wrong","status":"backlog"}]']
     base={'id':'a','title':'Task','description':'','priority':'low','status':'backlog'}
     invalid += [json.dumps([base,base]),json.dumps([dict(base,title=' '*3)]),json.dumps([dict(base,title='x'*81)]),json.dumps([dict(base,description='x'*281)]),json.dumps([dict(base,id='x'*81)]),json.dumps([dict(base,id='id'+str(i)) for i in range(201)])]
     for value in invalid:
      page.evaluate("v=>localStorage.setItem('urdia-launchboard-v1',v)",value);page.reload();seed();assert page.evaluate("localStorage.getItem('urdia-other-app-sentinel')")=='keep'
    record('malformed_storage_recovers_safely',malformed)
    def unavailable():
     p=context.new_page();p.add_init_script("Storage.prototype.getItem=()=>{throw new Error('unavailable')}; Storage.prototype.setItem=()=>{throw new Error('unavailable')}");p.goto(origin+'/');expect(p.locator('.task-card')).to_have_count(6);p.locator('#new-task').click();p.locator('#task-title').fill('Without storage');p.locator('#save-task').click();expect(p.locator('.task-card')).to_have_count(7);expect(p.locator('#task-dialog')).not_to_be_visible();warning=p.locator('[role="status"]:visible').filter(has_text='storage');expect(warning).not_to_have_count(0);p.close()
    record('unavailable_storage_does_not_break_board',unavailable)
    assert not external and not errors
    record('no_external_requests_or_uncaught_errors',lambda:None)
    page.set_viewport_size({'width':1440,'height':1000});reset();page.screenshot(path=str(output/'desktop-final.png'),full_page=True)
    context.close()
   finally:browser.close()
 finally:
  server.shutdown();server.server_close();thread.join(timeout=2)
  report={'schema':'urdia.launchboard-checks/v1','checks':results,'passed':len(results)==15 and all(r['passed'] for r in results),'source_sha256':before,'source_unchanged':all(hashlib.sha256((workspace/n).read_bytes()).hexdigest()==h for n,h in before.items()),'external_requests':len(external),'uncaught_errors':len(errors)}
  (output/'checks.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
 return report


if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('workspace',type=Path);p.add_argument('output',type=Path);a=p.parse_args();r=check(a.workspace,a.output);raise SystemExit(0 if r['passed'] and r['source_unchanged'] else 1)
