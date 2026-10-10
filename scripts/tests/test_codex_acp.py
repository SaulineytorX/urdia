"""Offline protocol and fail-closed regressions. Never starts installed Codex."""
import asyncio
from contextlib import redirect_stderr
import io
from unittest.mock import patch
import importlib.util
import json
import os
import signal
import sys
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('codex_bridge', ROOT / 'scripts/codex-acp.py')
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)
probe_spec = importlib.util.spec_from_file_location("codex_browser", ROOT / "scripts/check-codex-browser.py")
probe = importlib.util.module_from_spec(probe_spec)
probe_spec.loader.exec_module(probe)


class BrowserProbeSafetyTests(unittest.TestCase):
    def test_real_account_requires_explicit_opt_in_before_paths_or_browser_import(self):
        arguments = ['probe', '--urdia', '/does-not-exist', '--codex', '/does-not-exist',
                     '--model', 'fixture', '--chromium', '/does-not-exist', '--output', '/does-not-exist']
        with patch.object(sys, 'argv', arguments), redirect_stderr(io.StringIO()) as error:
            with self.assertRaises(SystemExit) as result:
                probe.main()
        self.assertEqual(result.exception.code, 2)
        self.assertIn('--live-account is required', error.getvalue())

    def test_dashboard_url_must_be_ip_loopback_with_bounded_port_and_token(self):
        token = 'a' * 64
        valid = ('http://127.0.0.1:12345/#token=' + token).encode()
        self.assertEqual(probe.private_url(valid), valid.decode())
        for raw in [valid.replace(b'127.0.0.1', b'0.0.0.0'), valid.replace(b'12345', b'99999'),
                    valid.replace(b'12345', b'0'), valid[:-1]]:
            with self.assertRaises(ValueError): probe.private_url(raw)

    def test_probe_never_allows_execution_or_unbounded_edits(self):
        self.assertEqual(probe.decision('edit', True, 0), 'allow_once')
        self.assertEqual(probe.decision('edit', True, 1), 'allow_once')
        for kind, allow, count in [('execute', True, 0), ('other', True, 0), ('edit', False, 0),
                                   ('edit', True, 2), ('edit', True, -1)]:
            self.assertEqual(probe.decision(kind, allow, count), 'reject_once')


class BridgeTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.b = bridge.Bridge('unused')
        self.b.thread_id = 'thread'
        self.b.turn_id = 'turn'
        self.b.turn_done = asyncio.get_running_loop().create_future()
        self.native, self.client = [], []
        async def send(v): self.native.append(v)
        async def emit(v): self.client.append(v)
        self.b.native_send, self.b.emit = send, emit

    def request(self, **changes):
        params = dict(threadId='thread', turnId='turn', itemId='tool', command='private-command')
        params.update(changes)
        return dict(id=19, method='item/commandExecution/requestApproval', params=params)

    async def test_only_exact_one_shot_option_grants(self):
        for option, expected in [('allow-once', 'accept'), ('allow-always', 'decline'), ('reject-once', 'decline')]:
            task = asyncio.create_task(self.b.approval(self.request()))
            await asyncio.sleep(0)
            self.assertEqual(len(self.b.client_pending), 1)
            future = next(iter(self.b.client_pending.values()))
            future.set_result({'outcome': {'outcome': 'selected', 'optionId': option}})
            await task
            self.assertEqual(self.native[-1]['result']['decision'], expected)
            self.assertEqual([x['kind'] for x in self.client[-1]['params']['options']], ['allow_once', 'reject_once'])

    async def test_scope_expansions_and_missing_file_item_deny(self):
        for changes in [dict(turnId='other'), dict(threadId='other'), dict(grantRoot='/'),
                        dict(networkApprovalContext={'host':'private'}), dict(kind='stdin'), dict(command=None)]:
            await self.b.approval(self.request(**changes))
            self.assertEqual(self.native[-1]['result']['decision'], 'decline')
        request = self.request(); request['method'] = 'item/fileChange/requestApproval'
        await self.b.approval(request)
        self.assertEqual(self.native[-1]['result']['decision'], 'decline')
        self.assertEqual(self.client, [])

    async def test_cancellation_race_denies_pending_permission(self):
        task = asyncio.create_task(self.b.approval(self.request()))
        await asyncio.sleep(0)
        async def rpc(method, params): return {}
        self.b.native_call = rpc
        await self.b.interrupt()
        await task
        self.assertEqual(self.native[-1]['result']['decision'], 'decline')
        self.assertTrue(self.b.cancel_requested)

    async def test_cancel_before_turn_id_is_remembered(self):
        self.b.turn_id = None
        calls = []
        async def rpc(method, params): calls.append((method, params)); return {}
        self.b.native_call = rpc
        await self.b.interrupt()
        await self.b.notification('turn/started', {'threadId':'thread','turn':{'id':'new'}})
        await asyncio.gather(*list(self.b.tasks))
        self.assertEqual(calls[0][0], 'turn/interrupt')
        self.assertFalse(self.b.scoped({'threadId':'thread','turnId':'new'}))

    async def test_unknown_callbacks_never_grant(self):
        await self.b.approval(dict(id=3, method='item/tool/call', params={}))
        self.assertIn('error', self.native[-1])
        await self.b.approval(dict(id=4, method='item/permissions/requestApproval', params={}))
        self.assertEqual(self.native[-1]['result']['permissions'], {})

    async def test_text_and_tool_output_omit_private_native_fields(self):
        await self.b.notification('item/started', {'threadId':'thread','turnId':'other','item':{'id':'ignored','type':'commandExecution'}})
        await self.b.notification('item/started', {'threadId':'thread','turnId':'turn','item':{'id':'tool','type':'commandExecution','status':'inProgress','command':'SECRET','cwd':'SECRET','aggregatedOutput':'SECRET'}})
        await self.b.notification('item/agentMessage/delta', {'threadId':'thread','turnId':'turn','itemId':'message','delta':'visible'})
        self.assertEqual(len(self.client), 2)
        self.assertNotIn('SECRET', json.dumps(self.client))
        await self.b.notification('turn/completed', {'threadId':'thread','turn':{'id':'turn','status':'failed'}})
        self.assertEqual(await self.b.turn_done, 'failed')
        await self.b.notification('item/agentMessage/delta', {'threadId':'thread','turnId':'turn','itemId':'message','delta':'late'})
        self.assertEqual(len(self.client), 2)

    async def test_authoritative_final_message_supplies_missing_tail_without_duplicate_text(self):
        final='{"objective":"fixture","steps":[{"title":"one","scope":"file","acceptance":"check"}]}'
        await self.b.notification('item/agentMessage/delta', {'threadId':'thread','turnId':'turn','itemId':'message','delta':final[:-2]})
        complete={'threadId':'thread','turnId':'turn','item':{'id':'message','type':'agentMessage','text':final}}
        await self.b.notification('item/completed',complete)
        text=''.join(frame['params']['update']['content']['text'] for frame in self.client)
        self.assertEqual(text,final)
        await self.b.notification('item/completed',complete)
        self.assertEqual(len(self.client),2)

    async def delta(self, text, item="message"):
        await self.b.notification('item/agentMessage/delta', dict(threadId='thread', turnId='turn', itemId=item, delta=text))

    async def final(self, text, item="message"):
        await self.b.notification('item/completed', dict(threadId='thread', turnId='turn', item=dict(id=item, type='agentMessage', text=text)))

    async def test_exact_and_final_only_sequential_unicode_messages(self):
        for chunk in ['hé', 'llo']: await self.delta(chunk)
        await self.final('héllo')
        await self.final('世界', 'second')
        await self.b.notification('turn/completed', dict(threadId='thread', turn=dict(id='turn', status='completed')))
        self.assertEqual(await self.b.turn_done, 'completed')
        self.assertEqual(''.join(f['params']['update']['content']['text'] for f in self.client), 'héllo世界')
        await self.final('late', 'third')
        self.assertEqual(len(self.client), 3)

    async def test_conflicting_final_missing_final_and_late_delta_fail(self):
        await self.delta('prefix')
        for text in ['different', 'pre', None]:
            with self.assertRaises(ValueError): await self.final(text)
        with self.assertRaises(ValueError):
            await self.b.notification('turn/completed', dict(threadId='thread', turn=dict(id='turn', status='completed')))
        self.assertFalse(self.b.turn_done.done())
        await self.final('prefix')
        with self.assertRaises(ValueError): await self.final('prefix changed')
        with self.assertRaises(ValueError): await self.delta('late')
        self.assertEqual(len(self.client), 1)

    async def test_interleaving_and_text_bounds_fail_before_forwarding(self):
        await self.delta('a')
        with self.assertRaises(ValueError): await self.delta('b', 'second')
        with patch.object(bridge, 'MAX_TURN_BYTES', 2):
            with self.assertRaises(ValueError): await self.final('aé')
        with patch.object(bridge, 'MAX_ITEMS', len(self.b.messages)):
            with self.assertRaises(ValueError): await self.final('c', 'third')
        with self.assertRaises(ValueError): await self.delta('text', '')
        self.assertEqual(len(self.client), 1)

    async def test_wrong_scope_is_ignored_and_malformed_json_is_never_repaired(self):
        for changes in [dict(threadId='other'), dict(turnId='other')]:
            params=dict(threadId='thread', turnId='turn', item=dict(id='message', type='agentMessage', text='PRIVATE'))
            params.update(changes)
            await self.b.notification('item/completed', params)
        self.assertEqual(self.client, [])
        await self.delta('{"steps":[')
        await self.final('{"steps":[')
        self.assertEqual(self.client[0]['params']['update']['content']['text'], '{"steps":[')
        self.assertEqual(len(self.client), 1)

    def test_native_failure_categories_use_pinned_structured_fields_not_messages(self):
        for info,expected in [('unauthorized','authentication'),('usageLimitExceeded','rate_or_quota'),
                              ('contextWindowExceeded','context_limit'),('badRequest','unknown')]:
            self.assertEqual(bridge.native_failure_kind({'codexErrorInfo':info,'message':'PRIVATE_NATIVE_SENTINEL unauthorized'}),expected)
        for status,expected in [(401,'authentication'),(403,'access_denied'),(429,'rate_or_quota'),(503,'connection')]:
            self.assertEqual(bridge.native_failure_kind({'codexErrorInfo':{'httpConnectionFailed':{'httpStatusCode':status}}}),expected)
        for value in [None,{'message':'401 unauthorized PRIVATE_NATIVE_SENTINEL'}, {'codexErrorInfo':{'unknown':{'httpStatusCode':401}}},
                      {'codexErrorInfo':{'httpConnectionFailed':{'httpStatusCode':True}}}]:
            self.assertIn(bridge.native_failure_kind(value),['unknown','connection'])

    async def test_native_failed_turn_keeps_only_category_and_does_not_retain_raw_details(self):
        await self.b.notification('turn/completed', {'threadId':'other','turn':{'id':'turn','status':'failed','error':{'codexErrorInfo':'unauthorized'}}})
        self.assertEqual(self.b.turn_failure_kind,'unknown')
        await self.b.notification('turn/completed', {'threadId':'thread','turn':{'id':'turn','status':'failed','error':{'codexErrorInfo':'unauthorized','message':'PRIVATE_NATIVE_SENTINEL'}}})
        self.assertEqual(await self.b.turn_done,'failed')
        self.assertEqual(self.b.turn_failure_kind,'authentication')
        self.assertNotIn('PRIVATE_NATIVE_SENTINEL',repr(self.b.__dict__))

    async def test_failed_native_prompt_emits_only_allowlisted_kind_and_clears_stale_diagnosis(self):
        self.b.turn_done = None
        async def rpc(method, params):
            if method == 'turn/start':
                await self.b.notification('turn/started', {'threadId':'thread','turn':{'id':'new-turn'}})
                await self.b.notification('turn/completed', {'threadId':'thread','turn':{'id':'new-turn','status':'failed','error':{'codexErrorInfo':'unauthorized','message':'PRIVATE_NATIVE_SENTINEL'}}})
                return {'turn':{'id':'new-turn'}}
            return {}
        self.b.native_call=rpc
        params={'sessionId':self.b.session_id,'prompt':[{'type':'text','text':'fixture'}]}
        await self.b.handle({'id':1,'method':'session/prompt','params':params})
        self.assertEqual(self.client[-1]['error']['data'],{'urdia_failure_kind':'authentication'})
        self.assertNotIn('PRIVATE_NATIVE_SENTINEL',json.dumps(self.client))
        await self.b.handle({'id':2,'method':'session/prompt','params':params})
        self.assertEqual(self.client[-1]['error']['data'],{'urdia_failure_kind':'unknown'})

    async def test_failed_bridge_rejects_future_work_without_private_diagnostics(self):
        self.b.fail()
        with self.assertRaises(ValueError): await self.b.turn_done
        await self.b.handle({'id':1,'method':'session/prompt','params':{'sessionId':self.b.session_id,'prompt':[None]}})
        self.assertIn('error', self.client[-1])

    def test_frame_and_identifier_limits(self):
        for value in [[], {'id':[]}, {'id':True}, {'method':False}]:
            with self.assertRaises(ValueError): bridge.validate_frame(value)
        with self.assertRaises(ValueError): bridge.encode({'text':'x'*bridge.MAX_FRAME})
        with self.assertRaises(ValueError): bridge.identifier('a\n')

    async def test_bridge_sigterm_reaps_native_child(self):
        with tempfile.TemporaryDirectory(prefix='urdia-codex-lifecycle-') as tmp:
            fake = Path(tmp)/'codex'
            marker = Path(tmp)/'pid'
            fake.write_text("#!/usr/bin/python3\nimport os,sys,json\nif '--version' in sys.argv:\n print('codex-cli 0.159.2');sys.exit(0)\nopen(" + repr(str(marker)) + ", 'w').write(str(os.getpid()))\nfor line in sys.stdin:\n m=json.loads(line)\n if m.get('method')=='initialize': print(json.dumps({'id':m['id'],'result':{}}),flush=True)\n")
            fake.chmod(0o700)
            process = await asyncio.create_subprocess_exec(sys.executable, '-I', '-B', str(ROOT/'scripts/codex-acp.py'), '--codex', str(fake), stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
            try:
                process.stdin.write(bridge.encode({'id':1,'method':'initialize','params':{'protocolVersion':1}}))
                await process.stdin.drain()
                frame = json.loads(await asyncio.wait_for(process.stdout.readline(),5))
                self.assertIn('result',frame)
                child = int(marker.read_text())
                process.send_signal(signal.SIGTERM)
                await asyncio.wait_for(process.wait(),7)
                with self.assertRaises(ProcessLookupError): os.kill(child,0)
                self.assertEqual(await process.stderr.read(),b'')
            finally:
                if process.returncode is None:
                    process.kill(); await process.wait()
                if marker.exists():
                    try: os.kill(int(marker.read_text()), signal.SIGKILL)
                    except ProcessLookupError: pass

    async def test_real_stdio_handshake_prompt_and_process_cleanup_offline(self):
        with tempfile.TemporaryDirectory(prefix='urdia-codex-fixture-') as tmp:
            fake = Path(tmp)/'codex'
            fake.write_text('''#!/usr/bin/python3
import json,sys
if '--version' in sys.argv:
 print('codex-cli 0.159.2');sys.exit(0)
def emit(v): print(json.dumps(v),flush=True)
for line in sys.stdin:
 m=json.loads(line);method=m.get('method');i=m.get('id')
 if method=='initialize':emit({'id':i,'result':{}})
 elif method=='thread/start':emit({'id':i,'result':{'thread':{'id':'thread'},'model':'fake-model','reasoningEffort':'low','approvalPolicy':'on-request','approvalsReviewer':'user','sandbox':{'type':'readOnly'}}})
 elif method=='turn/start':
  emit({'method':'turn/started','params':{'threadId':'thread','turn':{'id':'turn'}}})
  emit({'id':i,'result':{'turn':{'id':'turn'}}})
  emit({'method':'item/agentMessage/delta','params':{'threadId':'thread','turnId':'turn','itemId':'message','delta':'FIXTURE_OK'}})
  emit({'method':'item/completed','params':{'threadId':'thread','turnId':'turn','item':{'id':'message','type':'agentMessage','text':'FIXTURE_OK'}}})
  emit({'method':'turn/completed','params':{'threadId':'thread','turn':{'id':'turn','status':'completed'}}})
''')
            fake.chmod(0o700)
            b=bridge.Bridge(str(fake),'fake-model','low'); frames=[]
            async def emit(v): frames.append(v)
            b.emit=emit
            try:
                await b.handle({'id':1,'method':'initialize','params':{'protocolVersion':1}})
                await b.handle({'id':2,'method':'session/new','params':{'cwd':tmp,'mcpServers':[]}})
                await b.handle({'id':3,'method':'session/prompt','params':{'sessionId':b.session_id,'prompt':[{'type':'text','text':'offline'}]}})
                self.assertEqual(frames[-1]['result']['stopReason'],'end_turn')
                self.assertIn('FIXTURE_OK',json.dumps(frames))
            finally: await b.close()
            self.assertIsNotNone(b.process.returncode)


if __name__ == '__main__': unittest.main()
