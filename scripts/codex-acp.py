#!/usr/bin/env python3
"""Experimental ACP v1 bridge to installed Codex 0.159.2 app-server over stdio.

No credentials are read by this bridge. Codex owns authentication and inference.
Only command/file-change one-shot approvals are supported; unknown requests deny.
"""
import argparse
import asyncio
import json
import os
from pathlib import Path
import signal
import sys
import uuid

MAX_FRAME = 1024 * 1024
MAX_TURN_BYTES = 8 * MAX_FRAME
MAX_TURN_MESSAGES = 4096
MAX_ITEMS = 128
SUPPORTED_VERSION = "codex-cli 0.159.2"


class ProviderFailure(ValueError):
    def __init__(self, kind):
        super().__init__("Native provider failure; private details omitted")
        self.kind = kind


def native_failure_kind(error):
    """Only the pinned native structured error vocabulary is diagnostic input."""
    if not isinstance(error, dict):
        return "unknown"
    info = error.get("codexErrorInfo")
    if isinstance(info, str):
        return {"unauthorized": "authentication", "usageLimitExceeded": "rate_or_quota",
                "rateLimitExceeded": "rate_or_quota", "sessionBudgetExceeded": "rate_or_quota",
                "contextWindowExceeded": "context_limit", "serverOverloaded": "connection"}.get(info, "unknown")
    if isinstance(info, dict) and len(info) == 1:
        variant, details = next(iter(info.items()))
        if variant in ("httpConnectionFailed", "responseStreamConnectionFailed",
                       "responseStreamDisconnected", "responseTooManyFailedAttempts") and isinstance(details, dict):
            status = details.get("httpStatusCode")
            if type(status) is int:
                kind = {401: "authentication", 403: "access_denied", 429: "rate_or_quota"}.get(status)
                if kind:
                    return kind
            return "connection"
    return "unknown"


def identifier(value):
    if not isinstance(value, str) or not 0 < len(value) <= 1024 or any(ord(c) < 32 or ord(c) > 126 for c in value):
        raise ValueError("Invalid protocol identifier")
    return value


def encode(value):
    data = (json.dumps(value, ensure_ascii=True, separators=(",", ":")) + "\n").encode()
    if len(data) > MAX_FRAME:
        raise ValueError("Protocol frame limit")
    return data


def tool_projection(item):
    if not isinstance(item, dict):
        raise ValueError("Invalid tool item")
    kind = {"commandExecution": "execute", "fileChange": "edit", "mcpToolCall": "other",
            "dynamicToolCall": "other", "webSearch": "search"}.get(item.get("type"))
    if kind is None:
        return None
    status = {"inProgress": "in_progress", "completed": "completed", "failed": "failed",
              "declined": "failed", "interrupted": "failed"}.get(item.get("status"), "pending")
    return kind, status


class Bridge:
    def __init__(self, codex, model=None, effort=None):
        self.codex, self.model, self.effort = codex, model, effort
        self.process = None
        self.reader_task = None
        self.pending = {}
        self.client_pending = {}
        self.sequence = 0
        self.session_id = str(uuid.uuid4())
        self.thread_id = None
        self.turn_id = None
        self.turn_done = None
        self.controls = []
        self.items = {}
        self.messages = {}
        self.last_text_item = None
        self.message_bytes = 0
        self.tasks = set()
        self.turn_bytes = 0
        self.turn_messages = 0
        self.client_requests = 0
        self.initialized = False
        self.cancel_requested = False
        self.turn_failure_kind = "unknown"
        self.failed = False
        self.write_lock = asyncio.Lock()

    async def emit(self, value):
        async with self.write_lock:
            sys.stdout.buffer.write(encode(value))
            sys.stdout.buffer.flush()

    async def native_send(self, value):
        self.process.stdin.write(encode(value))
        await self.process.stdin.drain()

    async def native_call(self, method, params):
        if self.failed or len(self.pending) >= 8:
            raise ValueError("Native request limit")
        self.sequence += 1
        request_id = self.sequence
        future = asyncio.get_running_loop().create_future()
        self.pending[request_id] = future
        try:
            await self.native_send({"id": request_id, "method": method, "params": params})
            return await asyncio.wait_for(future, 25)
        except (asyncio.TimeoutError, OSError):
            self.fail()
            raise
        finally:
            self.pending.pop(request_id, None)
            if future.done() and not future.cancelled():
                future.exception()

    async def start(self):
        version = await asyncio.create_subprocess_exec(self.codex, "--version", stdout=asyncio.subprocess.PIPE,
                                                      stderr=asyncio.subprocess.DEVNULL)
        try:
            raw = await asyncio.wait_for(version.stdout.read(4097), 10)
            await asyncio.wait_for(version.wait(), 10)
            if len(raw) > 4096:
                raise ValueError("Version output limit")
        except BaseException:
            if version.returncode is None:
                version.kill()
                await version.wait()
            raise
        if version.returncode or raw.decode().strip() != SUPPORTED_VERSION:
            raise ValueError("Unsupported Codex version")
        self.process = await asyncio.create_subprocess_exec(
            self.codex, "app-server", "--listen", "stdio://",
            "-c", 'sandbox_mode="read-only"', "-c", 'approval_policy="on-request"',
            "-c", 'approvals_reviewer="user"', "-c", "features.multi_agent=false",
            stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.DEVNULL, limit=MAX_FRAME + 1, start_new_session=True)
        self.reader_task = asyncio.create_task(self.native_reader())
        await self.native_call("initialize", {"clientInfo": {"name": "urdia", "title": "Urdia", "version": "0.1.0"},
                                              "capabilities": {"experimentalApi": False}})
        await self.native_send({"method": "initialized"})

    def scoped(self, params):
        return (not self.cancel_requested and not self.failed and self.turn_done is not None and not self.turn_done.done()
                and params.get("threadId") == self.thread_id
                and self.turn_id is not None and params.get("turnId") == self.turn_id)

    async def update(self, update):
        await self.emit({"jsonrpc": "2.0", "method": "session/update",
                         "params": {"sessionId": self.session_id, "update": update}})

    def message_state(self, item_id):
        item_id = identifier(item_id)
        if item_id not in self.messages:
            if len(self.messages) >= MAX_ITEMS:
                raise ValueError("Text item limit")
            self.messages[item_id] = {"text": "", "completed": False}
        return item_id, self.messages[item_id]

    async def append_message_text(self, item_id, state, text):
        if not isinstance(text, str):
            raise ValueError("Invalid message text")
        if not text:
            return
        # ACP exposes append-only text. Never reorder or rewrite an earlier item.
        if self.last_text_item != item_id and self.last_text_item is not None:
            previous = self.messages[self.last_text_item]
            if not previous["completed"] or state["text"]:
                raise ValueError("Interleaved text is unsupported")
        byte_count = len(text.encode())
        if self.message_bytes + byte_count > MAX_TURN_BYTES:
            raise ValueError("Text byte limit")
        await self.update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}})
        state["text"] += text
        self.message_bytes += byte_count
        self.last_text_item = item_id

    async def finish_message(self, item):
        item_id, state = self.message_state(item.get("id"))
        final = item.get("text")
        if not isinstance(final, str) or not final.startswith(state["text"]):
            raise ValueError("Final message disagrees with streamed text")
        if state["completed"]:
            if final != state["text"]:
                raise ValueError("Completed message changed")
            return
        # Copy a suffix from native final state, never invent delimiters or repair JSON.
        await self.append_message_text(item_id, state, final[len(state["text"]):])
        state["completed"] = True

    async def notification(self, method, params):
        if not isinstance(params, dict) or params.get("threadId") != self.thread_id or self.turn_done is None or self.turn_done.done():
            return
        if method == "turn/started" and self.turn_id is None:
            self.turn_id = identifier(params["turn"]["id"])
            if self.cancel_requested:
                task = asyncio.create_task(self.interrupt())
                self.track(task)
            return
        if method == "turn/completed":
            turn = params.get("turn", {})
            if turn.get("id") == self.turn_id:
                if turn.get("status") == "failed":
                    self.turn_failure_kind = native_failure_kind(turn.get("error"))
                if (turn.get("status") == "completed" and not self.cancel_requested
                        and any(not message["completed"] for message in self.messages.values())):
                    raise ValueError("Message final state missing")
                self.turn_done.set_result(turn.get("status"))
            return
        if not self.scoped(params):
            return
        if method == "item/agentMessage/delta":
            delta = params.get("delta")
            if not isinstance(delta, str):
                raise ValueError("Invalid text delta")
            item_id, state = self.message_state(params.get("itemId"))
            if state["completed"]:
                raise ValueError("Text arrived after message completion")
            await self.append_message_text(item_id, state, delta)
        elif method in ("item/started", "item/completed"):
            item = params.get("item", {})
            if isinstance(item, dict) and item.get("type") == "agentMessage":
                if method == "item/completed":
                    await self.finish_message(item)
                else:
                    self.message_state(item.get("id"))
                return
            projection = tool_projection(item)
            if projection:
                item_id = identifier(item["id"])
                # Permission correlation is bounded. ACP independently bounds projected tool telemetry.
                if item_id in self.items or len(self.items) < MAX_ITEMS:
                    self.items[item_id] = item
                kind, status = projection
                await self.update({"sessionUpdate": "tool_call", "toolCallId": item_id,
                                   "title": "Codex reported tool activity", "kind": kind, "status": status})

    async def approval(self, message):
        method, params, request_id = message["method"], message.get("params", {}), message["id"]
        decision = "decline"
        supported = method in ("item/commandExecution/requestApproval", "item/fileChange/requestApproval")
        if method == "item/permissions/requestApproval":
            await self.native_send({"id": request_id, "result": {"permissions": {}, "scope": "turn"}})
            return
        if not supported:
            await self.native_send({"id": request_id, "error": {"code": -32601, "message": "Unsupported client request"}})
            return
        # Never grant session/root/network expansions. Only explicitly correlated one-shot requests.
        safe = (isinstance(params, dict) and self.scoped(params) and not params.get("grantRoot")
                and not params.get("networkApprovalContext") and params.get("kind", "command") == "command")
        kind = "execute" if "commandExecution" in method else "edit"
        item = self.items.get(params.get("itemId")) if isinstance(params, dict) else None
        safe = safe and ((kind == "execute" and isinstance(params.get("command"), str) and bool(params["command"]))
                         or (kind == "edit" and item is not None and item.get("type") == "fileChange"))
        if safe and len(self.client_pending) < 8:
            client_id = "approval-" + str(uuid.uuid4())
            future = asyncio.get_running_loop().create_future()
            self.client_pending[client_id] = future
            turn_id = self.turn_id
            try:
                await self.emit({"jsonrpc": "2.0", "id": client_id, "method": "session/request_permission", "params": {
                    "sessionId": self.session_id, "toolCall": {"toolCallId": identifier(params["itemId"]),
                        "kind": kind, "title": "Codex one-shot approval", "rawInput": {"request": params, "item": item}},
                    "options": [{"optionId": "allow-once", "name": "Allow once", "kind": "allow_once"},
                                {"optionId": "reject-once", "name": "Reject once", "kind": "reject_once"}]}})
                result = await asyncio.wait_for(future, 60)
                outcome = result.get("outcome", {}) if isinstance(result, dict) else {}
                if (self.scoped(params) and self.turn_id == turn_id and outcome.get("outcome") == "selected"
                        and outcome.get("optionId") == "allow-once"):
                    decision = "accept"
            except (ValueError, asyncio.TimeoutError):
                pass
            finally:
                self.client_pending.pop(client_id, None)
        await self.native_send({"id": request_id, "result": {"decision": decision}})

    def fail(self):
        self.failed = True
        self.cancel_requested = True
        for future in [*self.pending.values(), self.turn_done]:
            if future is not None and not future.done():
                future.set_exception(ValueError("Native stream failed; details omitted"))
        for future in self.client_pending.values():
            if not future.done():
                future.set_result({"outcome": {"outcome": "cancelled"}})
        if self.process and self.process.returncode is None:
            try:
                os.killpg(self.process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass

    def track(self, task):
        self.tasks.add(task)
        def finished(completed):
            self.tasks.discard(completed)
            if not completed.cancelled() and completed.exception() is not None:
                self.fail()
        task.add_done_callback(finished)

    async def native_reader(self):
        try:
            while True:
                line = await self.process.stdout.readline()
                if not line or len(line) > MAX_FRAME:
                    raise ValueError("Native stream ended or exceeded limit")
                if self.turn_done is not None and not self.turn_done.done():
                    self.turn_bytes += len(line)
                    self.turn_messages += 1
                    if self.turn_bytes > MAX_TURN_BYTES or self.turn_messages > MAX_TURN_MESSAGES:
                        raise ValueError("Turn telemetry limit")
                message = json.loads(line)
                validate_frame(message)
                if "method" in message and "id" in message:
                    if len(self.tasks) >= 16:
                        raise ValueError("Native callback limit")
                    task = asyncio.create_task(self.approval(message))
                    self.track(task)
                elif "method" in message:
                    await self.notification(message["method"], message.get("params", {}))
                elif message.get("id") in self.pending:
                    future = self.pending[message["id"]]
                    if not future.done():
                        if "error" in message:
                            future.set_exception(ProviderFailure(native_failure_kind(message["error"])))
                        else:
                            future.set_result(message.get("result", {}))
        except (ValueError, KeyError, TypeError, OSError, asyncio.LimitOverrunError):
            self.fail()

    async def handle(self, message):
        request_id = message.get("id")
        method, params = message.get("method"), message.get("params", {})
        if request_id is None and method != "session/cancel":
            return
        try:
            if self.failed or not isinstance(params, dict):
                raise ValueError("Invalid parameters")
            if method == "initialize":
                if self.initialized or params.get("protocolVersion") != 1:
                    raise ValueError("Unsupported initialization")
                self.initialized = True
                await self.start()
                result = {"protocolVersion": 1, "agentCapabilities": {}, "authMethods": [],
                          "agentInfo": {"name": "urdia-codex-bridge", "version": "0.1.0"}}
            elif method == "session/new":
                if not self.initialized or self.thread_id:
                    raise ValueError("Only one session is supported")
                cwd = str(Path(params["cwd"]).resolve(strict=True))
                if not Path(cwd).is_dir() or params.get("mcpServers"):
                    raise ValueError("Unsupported workspace or MCP servers")
                config = {"features.multi_agent": False, "web_search": "disabled"}
                if self.effort:
                    config["model_reasoning_effort"] = self.effort
                native = await self.native_call("thread/start", {"cwd": cwd, "model": self.model,
                    "approvalPolicy": "on-request", "approvalsReviewer": "user", "sandbox": "read-only",
                    "ephemeral": True, "config": config})
                if (native.get("approvalPolicy") != "on-request" or native.get("approvalsReviewer") != "user"
                        or native.get("sandbox", {}).get("type") != "readOnly"):
                    raise ValueError("Native permission settings unconfirmed")
                self.thread_id = identifier(native["thread"]["id"])
                for category, value, desired in [("model", native.get("model"), self.model),
                                                  ("thought_level", native.get("reasoningEffort"), self.effort)]:
                    if desired and desired != value:
                        raise ValueError("Requested controls unconfirmed")
                    if value is not None:
                        value = identifier(value)
                        self.controls.append({"id": category, "name": category, "category": category, "type": "select",
                                              "currentValue": value, "options": [{"value": value, "name": value}]})
                result = {"sessionId": self.session_id, "configOptions": self.controls}
            else:
                if not self.thread_id or params.get("sessionId") != self.session_id:
                    raise ValueError("Session mismatch")
                if method == "session/prompt":
                    if self.turn_done is not None:
                        raise ValueError("Prompt already active")
                    prompt = params.get("prompt")
                    if (not isinstance(prompt, list) or not 0 < len(prompt) <= 16
                            or any(not isinstance(item, dict) or item.get("type") != "text" or not isinstance(item.get("text"), str) for item in prompt)):
                        raise ValueError("Only text input is supported")
                    text = "\n".join(item["text"] for item in prompt)
                    if not text.strip() or len(text) > 16384:
                        raise ValueError("Prompt limit")
                    self.turn_done = asyncio.get_running_loop().create_future()
                    self.turn_id = None
                    self.cancel_requested = False
                    self.turn_failure_kind = "unknown"
                    self.items = {}
                    self.messages = {}
                    self.last_text_item = None
                    self.message_bytes = 0
                    self.turn_bytes = self.turn_messages = 0
                    try:
                        native = await self.native_call("turn/start", {"threadId": self.thread_id,
                            "input": [{"type": "text", "text": text, "text_elements": []}]})
                        turn_id = identifier(native["turn"]["id"])
                        if self.turn_id is not None and self.turn_id != turn_id:
                            raise ValueError("Turn mismatch")
                        self.turn_id = turn_id
                        if self.cancel_requested:
                            await self.interrupt()
                        status = await asyncio.wait_for(self.turn_done, 240)
                        if status not in ("completed", "interrupted"):
                            raise ProviderFailure(self.turn_failure_kind)
                        result = {"stopReason": "end_turn" if status == "completed" else "cancelled"}
                    except BaseException:
                        try:
                            await self.interrupt()
                        finally:
                            self.fail()
                        raise
                    finally:
                        if self.turn_done.done() and not self.turn_done.cancelled():
                            self.turn_done.exception()
                        self.turn_done = None
                        self.turn_id = None
                        self.items = {}
                        self.messages = {}
                        self.last_text_item = None
                        self.message_bytes = 0
                elif method == "session/cancel":
                    await self.interrupt()
                    return
                elif method == "session/set_config_option":
                    if not any(option["id"] == params.get("configId") and option["currentValue"] == params.get("value") for option in self.controls):
                        raise ValueError("Live setting changes are unsupported")
                    result = {"configOptions": self.controls}
                else:
                    raise ValueError("Unsupported ACP method")
            await self.emit({"jsonrpc": "2.0", "id": request_id, "result": result})
        except (ValueError, KeyError, TypeError, OSError, asyncio.TimeoutError) as error:
            if request_id is not None:
                kind = error.kind if isinstance(error, ProviderFailure) else "timeout" if isinstance(error, asyncio.TimeoutError) else "unknown"
                await self.emit({"jsonrpc": "2.0", "id": request_id,
                                 "error": {"code": -32603, "message": "Codex bridge request failed; private diagnostics omitted",
                                           "data": {"urdia_failure_kind":kind}}})

    async def interrupt(self):
        self.cancel_requested = True
        for future in self.client_pending.values():
            if not future.done():
                future.set_result({"outcome": {"outcome": "cancelled"}})
        if self.turn_id and self.turn_done is not None and not self.turn_done.done():
            await self.native_call("turn/interrupt", {"threadId": self.thread_id, "turnId": self.turn_id})

    async def close(self):
        for task in list(self.tasks):
            task.cancel()
        if self.tasks:
            await asyncio.gather(*self.tasks, return_exceptions=True)
        if self.reader_task:
            self.reader_task.cancel()
            await asyncio.gather(self.reader_task, return_exceptions=True)
        if self.process:
            try:
                os.killpg(self.process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                await asyncio.wait_for(self.process.wait(), 5)
            except asyncio.TimeoutError:
                os.killpg(self.process.pid, signal.SIGKILL)
                await self.process.wait()

    async def run(self):
        reader = asyncio.StreamReader(limit=MAX_FRAME + 1)
        protocol = asyncio.StreamReaderProtocol(reader)
        await asyncio.get_running_loop().connect_read_pipe(lambda: protocol, sys.stdin.buffer)
        handlers = set()
        try:
            while True:
                line = await reader.readline()
                if not line:
                    break
                if len(line) > MAX_FRAME:
                    raise ValueError("ACP frame limit")
                message = json.loads(line)
                validate_frame(message)
                if message.get("id") in self.client_pending and "method" not in message:
                    future = self.client_pending[message["id"]]
                    if not future.done():
                        future.set_result(message.get("result", {}))
                else:
                    self.client_requests += 1
                    if len(handlers) >= 16 or self.client_requests > 2048:
                        raise ValueError("ACP request limit")
                    task = asyncio.create_task(self.handle(message))
                    handlers.add(task)
                    def finished(completed):
                        handlers.discard(completed)
                        if not completed.cancelled() and completed.exception() is not None:
                            self.fail()
                    task.add_done_callback(finished)
        finally:
            self.fail()
            for task in list(handlers):
                task.cancel()
            await asyncio.gather(*handlers, return_exceptions=True)
            await self.close()


def validate_frame(message):
    if not isinstance(message, dict):
        raise ValueError("Invalid protocol frame")
    if "id" in message and (isinstance(message["id"], bool) or not isinstance(message["id"], (str, int))):
        raise ValueError("Invalid request identifier")
    if isinstance(message.get("id"), str):
        identifier(message["id"])
    if "method" in message:
        identifier(message["method"])


async def serve(bridge):
    task = asyncio.current_task()
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGTERM, signal.SIGINT):
        loop.add_signal_handler(sig, task.cancel)
    try:
        await bridge.run()
    except asyncio.CancelledError:
        pass
    finally:
        for sig in (signal.SIGTERM, signal.SIGINT):
            loop.remove_signal_handler(sig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--codex", default="codex", help="Installed native Codex executable; no downloads")
    parser.add_argument("--model", type=identifier)
    parser.add_argument("--effort", choices=["none", "minimal", "low", "medium", "high", "xhigh"])
    args = parser.parse_args()
    try:
        asyncio.run(serve(Bridge(args.codex, args.model, args.effort)))
    except (ValueError, OSError, KeyboardInterrupt):
        print("Codex bridge stopped; private diagnostics omitted", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
