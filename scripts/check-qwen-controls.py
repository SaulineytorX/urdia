#!/usr/bin/env python3
"""Offline Qwen ACP configuration probe; no prompts or real credentials.

This checks agent-reported selectors, not model inference, performance or billing.
"""
import argparse
import json
import os
from pathlib import Path
import re
import selectors
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MAX_BYTES = 512 * 1024
MAX_MESSAGES = 256


def fail():
    return ValueError('ACP control probe failed; private agent diagnostics omitted')


def token(value):
    if not isinstance(value, str) or not value or len(value) > 1024 or any(ord(c) < 32 or ord(c) > 126 for c in value):
        raise fail()
    return value


def controls(result):
    """Keep only bounded model/thought select IDs and values; omit raw metadata."""
    raw = result.get('configOptions')
    if not isinstance(raw, list) or len(raw) > 64:
        raise fail()
    output = {}
    for item in raw:
        if not isinstance(item, dict):
            raise fail()
        if item.get('category') not in ('model', 'thought_level'):
            continue
        if item.get('type') != 'select':
            continue
        key = token(item.get('id'))
        if key in output:
            raise fail()
        options = item.get('options')
        if not isinstance(options, list) or len(options) > 64:
            raise fail()
        values = []
        for option in options:
            if not isinstance(option, dict):
                raise fail()
            group = option.get('options') if 'group' in option else [option]
            if not isinstance(group, list) or len(group) > 64:
                raise fail()
            for member in group:
                if not isinstance(member, dict):
                    raise fail()
                values.append(token(member.get('value')))
        if len(values) > 128 or len(set(values)) != len(values):
            raise fail()
        current = token(item.get('currentValue'))
        if current not in values:
            raise fail()
        output[key] = {'category': item['category'], 'current': current, 'values': values}
    return output


def option_by_category(snapshot, category):
    if category not in ('model', 'thought_level'):
        raise fail()
    matches = [(key, entry) for key, entry in snapshot.items() if entry['category'] == category]
    if len(matches) != 1:
        raise fail()
    return matches[0]


class Peer:
    def __init__(self, process):
        self.process = process
        self.buffer = b''
        self.sequence = 0
        self.bytes = 0
        self.messages = 0
        self.selector = selectors.DefaultSelector()
        self.selector.register(process.stdout, selectors.EVENT_READ)
        self.deadline = time.monotonic() + 45

    def send(self, value):
        self.process.stdin.write((json.dumps(value) + '\n').encode())
        self.process.stdin.flush()

    def call(self, method, params):
        if method not in ('initialize', 'session/new', 'session/set_config_option'):
            raise fail()
        self.sequence += 1
        self.send({'jsonrpc': '2.0', 'id': self.sequence, 'method': method, 'params': params})
        while time.monotonic() < self.deadline:
            if b'\n' not in self.buffer:
                if not self.selector.select(timeout=min(.2, max(0, self.deadline-time.monotonic()))):
                    continue
                data = os.read(self.process.stdout.fileno(), 16384)
                if not data:
                    raise fail()
                self.bytes += len(data)
                if self.bytes > MAX_BYTES:
                    raise fail()
                self.buffer += data
                continue
            line, self.buffer = self.buffer.split(b'\n', 1)
            self.messages += 1
            if self.messages > MAX_MESSAGES:
                raise fail()
            try:
                message = json.loads(line)
            except (ValueError, UnicodeError):
                raise fail() from None
            if not isinstance(message, dict) or message.get('jsonrpc') != '2.0':
                raise fail()
            if 'method' in message:
                if 'id' in message:
                    response = ({'result': {'outcome': {'outcome': 'cancelled'}}}
                                if message['method'] == 'session/request_permission' else
                                {'error': {'code': -32601, 'message': 'Client method unavailable'}})
                    self.send({'jsonrpc': '2.0', 'id': message['id'], **response})
                continue
            if message.get('id') != self.sequence or 'error' in message:
                raise fail()
            result = message.get('result')
            if not isinstance(result, dict):
                raise fail()
            return result
        raise fail()

    def set_value(self, session, snapshot, category, value):
        key, entry = option_by_category(snapshot, category)
        if value not in entry['values']:
            raise ValueError('requested value is not advertised; no ACP change sent')
        updated = controls(self.call('session/set_config_option',
                            {'sessionId': session, 'configId': key, 'value': value}))
        _, selected = option_by_category(updated, category)
        if selected['current'] != value:
            raise fail()
        return updated

    def close(self):
        self.selector.close()


def probe(qwen, node=None):
    command = [str(node), str(qwen)] if node else [str(qwen)]
    requests = []
    class Reject(BaseHTTPRequestHandler):
        def do_GET(self):
            requests.append('GET')
            self.send_error(503, 'Offline probe; inference disabled')
        def do_POST(self):
            requests.append('POST')
            self.send_error(503, 'Offline probe; inference disabled')
        def log_message(self, *args):
            pass
    server = ThreadingHTTPServer(('127.0.0.1', 0), Reject)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='urdia-qwen-controls-') as raw:
            root = Path(raw); home = root/'home'; project = root/'project'
            (home/'.qwen').mkdir(parents=True); project.mkdir()
            url = f'http://127.0.0.1:{server.server_port}/v1'
            settings = {'security': {'auth': {'selectedType': 'openai'}},
                        'modelProviders': {'openai': [
                            {'id': model, 'name': model, 'envKey': 'OPENAI_API_KEY', 'baseUrl': url}
                            for model in ('qwen3.8-max', 'qwen3.7-plus')]}}
            (home/'.qwen/settings.json').write_text(json.dumps(settings))
            env = {'HOME': str(home), 'PATH': os.environ.get('PATH', '/usr/bin:/bin'),
                   'LC_ALL': 'C', 'OPENAI_API_KEY': 'offline-placeholder-not-a-credential',
                   'OPENAI_BASE_URL': url, 'OPENAI_MODEL': 'qwen3.8-max',
                   'XDG_CONFIG_HOME': str(home/'.config'), 'XDG_CACHE_HOME': str(home/'.cache'),
                   'XDG_STATE_HOME': str(home/'.local/state')}
            version = subprocess.run([*command, '--version'], env=env, cwd=project,
                                     capture_output=True, timeout=10, check=False)
            if version.returncode or version.stdout.strip() != b'0.24.7':
                raise fail()
            with tempfile.TemporaryFile() as errors:
                process = subprocess.Popen([*command, '--acp', '--bare', '--safe-mode',
                    '--chat-recording=false', '--auth-type', 'openai', '--model', 'qwen3.8-max',
                    '--openai-base-url', url], cwd=project, env=env, stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE, stderr=errors, start_new_session=True)
                peer = None
                try:
                    peer = Peer(process)
                    initialized = peer.call('initialize', {'protocolVersion': 1, 'clientCapabilities': {},
                        'clientInfo': {'name': 'urdia-offline-control-probe', 'version': '1'}})
                    if initialized.get('protocolVersion') != 1:
                        raise fail()
                    session = peer.call('session/new', {'cwd': str(project), 'mcpServers': []})
                    session_id = session.get('sessionId')
                    if not isinstance(session_id, str) or not session_id or len(session_id) > 512:
                        raise fail()
                    initial = controls(session)
                    medium = peer.set_value(session_id, initial, 'thought_level', 'medium')
                    _, models = option_by_category(medium, 'model')
                    matches = [value for value in models['values'] if value == 'qwen3.7-plus' or value.startswith('qwen3.7-plus(')]
                    if len(matches) != 1:
                        raise fail()
                    changed = peer.set_value(session_id, medium, 'model', matches[0])
                    _, thinking = option_by_category(changed, 'thought_level')
                    if 'medium' in thinking['values']:
                        raise fail()
                    rejected_locally = False
                    try:
                        peer.set_value(session_id, changed, 'thought_level', 'medium')
                    except ValueError as error:
                        rejected_locally = str(error).startswith('requested value')
                    if not rejected_locally:
                        raise fail()
                    return {'schema': 'urdia-qwen-control-probe-v1', 'qwen_version': version.stdout.decode().strip(),
                            'evidence_level': 'agent-reported-session-configuration',
                            'initial': initial, 'after_medium': medium, 'after_model_switch': changed,
                            'unsupported_value_rejected_before_rpc': True,
                            'prompt_calls': 0, 'offline_endpoint_requests_rejected': len(requests),
                            'real_inference_verified': False, 'routing_quality_verified': False}
                finally:
                    if peer is not None:
                        peer.close()
                    if process.poll() is None:
                        os.killpg(process.pid, signal.SIGTERM)
                        try:
                            process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            os.killpg(process.pid, signal.SIGKILL); process.wait(timeout=5)
                    process.stdin.close(); process.stdout.close()
    finally:
        server.shutdown(); server.server_close(); thread.join(timeout=2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qwen', help='trusted installed Qwen 0.24.7 executable')
    parser.add_argument('--node', help='explicit Node executable when --qwen names cli.js')
    args = parser.parse_args()
    if sys.platform != 'linux':
        raise ValueError('run inside Linux/WSL')
    selected = args.qwen or shutil.which('qwen')
    if not selected:
        raise ValueError('Qwen executable unavailable')
    print(json.dumps(probe(Path(selected).resolve(strict=True), Path(args.node).resolve(strict=True) if args.node else None), indent=2))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.TimeoutExpired):
        print('Qwen control probe failed; private diagnostics omitted', file=sys.stderr)
        raise SystemExit(1)
