#!/usr/bin/env python3
"""Offline control-probe regressions; no installed Qwen or model API required."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('probe', ROOT/'scripts/check-qwen-controls.py')
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


def configuration(current='medium', values=None):
    return {'configOptions': [{'id': 'reasoning_effort', 'category': 'thought_level',
            'type': 'select', 'currentValue': current,
            'options': [{'value': value} for value in (values or ['none', 'medium'])],
            '_meta': {'PRIVATE_SENTINEL': 'PRIVATE_SENTINEL'}}]}


class ControlTests(unittest.TestCase):
    def test_grouped_values_opaque_ids_and_metadata_omission(self):
        raw = configuration()
        raw['configOptions'].append({'id': 'model', 'category': 'model', 'type': 'select',
            'currentValue': 'model(openai)', 'options': [{'group': 'available', 'options': [{'value': 'model(openai)'}]}]})
        result = probe.controls(raw)
        self.assertEqual(result['model']['values'], ['model(openai)'])
        self.assertNotIn('PRIVATE_SENTINEL', json.dumps(result))
        raw['configOptions'].append({'id': 'mode', 'category': 'mode', 'type': 'select',
                                    'currentValue': 'yolo', 'options': [{'value': 'yolo'}]})
        self.assertNotIn('mode', probe.controls(raw))
        with self.assertRaises(ValueError):
            probe.option_by_category({'mode': {'category': 'mode'}}, 'mode')

    def test_malformed_duplicate_or_unadvertised_values_fail_without_echo(self):
        for raw in [configuration('PRIVATE_SENTINEL'), configuration(values=['medium','medium']),
                    configuration(values=['bad\nvalue']), {'configOptions': [configuration()['configOptions'][0]] * 65},
                    {'configOptions': 'PRIVATE_SENTINEL'}, {'configOptions': [False]}]:
            with self.assertRaises(ValueError) as failure:
                probe.controls(raw)
            self.assertNotIn('PRIVATE_SENTINEL', str(failure.exception))
        raw = configuration(); raw['configOptions'] *= 2
        with self.assertRaises(ValueError): probe.controls(raw)

    def test_unsupported_effort_is_rejected_before_rpc(self):
        class NoRpc:
            def call(self, *args): raise AssertionError('RPC must not be sent')
        with self.assertRaisesRegex(ValueError, 'no ACP change sent'):
            probe.Peer.set_value(NoRpc(), 'session', probe.controls(configuration()), 'thought_level', 'max')
        class WrongConfirmation:
            def call(self, *args): return configuration(current='none')
        with self.assertRaises(ValueError):
            probe.Peer.set_value(WrongConfirmation(), 'session', probe.controls(configuration()), 'thought_level', 'medium')


    def test_peer_denies_client_requests_and_consumes_buffered_responses(self):
        code = r'''
import json,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'jsonrpc':'2.0','id':'agent-question','method':'session/request_permission','params':{}}),flush=True)
answer=json.loads(sys.stdin.readline())
assert answer['result']['outcome']['outcome']=='cancelled'
print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{}}),flush=True)
print(json.dumps({'jsonrpc':'2.0','id':request['id'],'result':{'ok':True}}),flush=True)
'''
        with subprocess.Popen([sys.executable,'-I','-u','-c',code],stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE,stderr=subprocess.DEVNULL) as process:
            peer = probe.Peer(process)
            try:
                self.assertEqual(peer.call('initialize', {}), {'ok': True})
                with self.assertRaises(ValueError): peer.call('session/prompt', {})
            finally:
                peer.close(); process.stdin.close(); process.stdout.close()
                process.wait(timeout=3)

    def test_response_byte_budget_and_error_are_redacted(self):
        for code, budget in [("import sys;sys.stdin.readline();sys.stdout.buffer.write(b'x'*2000);sys.stdout.flush()", 16),
                             ("import sys;sys.stdin.readline();print('{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"message\":\"PRIVATE_SENTINEL\"}}',flush=True)", probe.MAX_BYTES)]:
            with subprocess.Popen([sys.executable,'-I','-u','-c',code], stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE,stderr=subprocess.DEVNULL) as process:
                peer=probe.Peer(process); old=probe.MAX_BYTES; probe.MAX_BYTES=budget
                try:
                    with self.assertRaises(ValueError) as failure:
                        peer.call('initialize', {})
                    self.assertNotIn('PRIVATE_SENTINEL', str(failure.exception))
                finally:
                    probe.MAX_BYTES=old; peer.close(); process.stdin.close(); process.stdout.close()
                    process.wait(timeout=3)


if __name__ == '__main__':
    unittest.main(verbosity=2)
