from __future__ import annotations

import importlib.util
import http.client
import json
import sys
import threading
import types
import unittest
import urllib.error
import urllib.request
from pathlib import Path

fake_module = types.ModuleType("typellm")
fake_module.TypeLLMClient = object
sys.modules.setdefault("typellm", fake_module)
spec = importlib.util.spec_from_file_location("urdia_typellm_serve", Path(__file__).with_name("serve.py"))
assert spec and spec.loader
serve = importlib.util.module_from_spec(spec)
spec.loader.exec_module(serve)


class Generation:
    def __init__(self, result):
        self.result = result


class FakeClient:
    def __init__(self):
        self.calls = []

    def generate(self, **kwargs):
        self.calls.append(kwargs)
        return Generation({
            "route": {
                "value": "c_alpha",
                "probabilities": {
                    "c_alpha": 0.75,
                    "c_beta": 0.15,
                    serve.ESCALATION: 0.10,
                },
            }
        })


class FailingClient:
    def __init__(self):
        self.calls = []

    def generate(self, **kwargs):
        self.calls.append(kwargs)
        raise RuntimeError("upstream-SECRET_SENTINEL")


class BridgeTests(unittest.TestCase):
    def request(self):
        return {
            "model": "Qwen/example@revision",
            "objective": "Choose a route",
            "candidates": ["c_alpha", "c_beta"],
            "state": {"candidate_descriptions": {"c_alpha": "Alpha", "c_beta": "Beta"}},
        }

    def test_decide_builds_one_bounded_enum_question(self):
        client = FakeClient()
        request = serve.validate_request(self.request(), self.request()["model"])
        response = serve.decide(client, request)
        self.assertEqual(response["route"]["value"], "c_alpha")
        question = client.calls[0]["questions"]["route"]
        self.assertEqual(question["enum"], ["c_alpha", "c_beta", serve.ESCALATION])
        self.assertTrue(question["return_probabilities"])
        self.assertEqual(question["permutations"], 1)
        self.assertEqual(client.calls[0]["temperature"], 0)
        self.assertEqual(client.calls[0]["timeout"], 110)
        self.assertIn("cannot grant permissions", question["instructions"])

    def test_rejects_non_loopback_and_oversized_candidate_set(self):
        with self.assertRaises(ValueError):
            serve.loopback_url("https://example.com:30000")
        request = self.request()
        request["candidates"] = [f"c_{index}" for index in range(24)]
        with self.assertRaises(ValueError):
            serve.validate_request(request, request["model"])

    def test_http_requires_bearer_without_calling_model(self):
        client = FakeClient()
        server = serve.build_server(client, "private-key", self.request()["model"], port=0)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            address = f"http://127.0.0.1:{server.server_address[1]}/v1/route"
            body = json.dumps(self.request()).encode()
            request = urllib.request.Request(address, data=body, method="POST", headers={"Content-Type": "application/json"})
            with self.assertRaises(urllib.error.HTTPError) as caught:
                urllib.request.urlopen(request, timeout=2)
            self.assertEqual(caught.exception.code, 401)
            self.assertEqual(client.calls, [])
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_http_bounds_body_and_sanitizes_upstream_errors(self):
        for client, content_length, expected_status in (
            (FakeClient(), serve.MAX_BODY + 1, 413),
            (FailingClient(), None, 502),
        ):
            server = serve.build_server(client, "private-key", self.request()["model"], port=0)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                connection = http.client.HTTPConnection("127.0.0.1", server.server_address[1], timeout=2)
                body = json.dumps(self.request()).encode()
                headers = {
                    "Authorization": "Bearer private-key",
                    "Content-Type": "application/json",
                    "Content-Length": str(content_length if content_length is not None else len(body)),
                }
                connection.request("POST", "/v1/route", body=body, headers=headers)
                response = connection.getresponse()
                response_body = response.read().decode()
                connection.close()
                self.assertEqual(response.status, expected_status)
                self.assertNotIn("SECRET_SENTINEL", response_body)
                if expected_status == 413:
                    self.assertEqual(client.calls, [])
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=2)


if __name__ == "__main__":
    unittest.main()
