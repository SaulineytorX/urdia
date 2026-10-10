#!/usr/bin/env python3
"""Process-level regression tests for ``scripts/smoke-supervisor.py``.

These tests deliberately use real Linux processes.  The fixtures publish a
PID and ``/proc`` start time before they become long lived; cleanup can then
use a pidfd and will never signal a recycled numeric PID.  No fixture changes
the resource limits of this test process.
"""

from __future__ import annotations

import errno
import json
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
from dataclasses import dataclass
from typing import Optional


ROOT = pathlib.Path(__file__).resolve().parents[2]
SUPERVISOR = ROOT / "scripts" / "smoke-supervisor.py"
PYTHON = sys.executable
POLL_SECONDS = 0.02


def proc_info(pid: int) -> Optional[tuple[bytes, int]]:
    """Return ``(state, starttime)`` for a process with a stable identity."""

    try:
        raw = pathlib.Path(f"/proc/{pid}/stat").read_bytes()
    except OSError:
        return None
    closing_paren = raw.rfind(b")")
    if closing_paren < 0:
        return None
    fields = raw[closing_paren + 2 :].split()
    if len(fields) < 20:
        return None
    try:
        return fields[0], int(fields[19])
    except ValueError:
        return None


def proc_comm(pid: int) -> Optional[bytes]:
    try:
        raw = pathlib.Path(f"/proc/{pid}/stat").read_bytes()
    except OSError:
        return None
    opening_paren = raw.find(b"(")
    closing_paren = raw.rfind(b")")
    if opening_paren < 0 or closing_paren <= opening_paren:
        return None
    return raw[opening_paren + 1 : closing_paren]


def proc_parent_and_starttime(pid: int) -> Optional[tuple[int, int]]:
    try:
        raw = pathlib.Path(f"/proc/{pid}/stat").read_bytes()
    except OSError:
        return None
    closing_paren = raw.rfind(b")")
    if closing_paren < 0:
        return None
    fields = raw[closing_paren + 2 :].split()
    if len(fields) < 20:
        return None
    try:
        return int(fields[1]), int(fields[19])
    except ValueError:
        return None


def pidfd_for(pid: int, starttime: int) -> Optional[int]:
    info = proc_info(pid)
    if info is None or info[1] != starttime:
        return None
    try:
        return os.pidfd_open(pid, 0)
    except (FileNotFoundError, ProcessLookupError):
        return None


@dataclass
class OwnedProcess:
    """A process reference guarded by its pidfd and /proc start time."""

    pid: int
    starttime: int
    pidfd: int

    def same_process_is_present(self) -> bool:
        info = proc_info(self.pid)
        return info is not None and info[1] == self.starttime

    def send(self, signum: int) -> None:
        if not self.same_process_is_present():
            return
        try:
            signal.pidfd_send_signal(self.pidfd, signum)
        except (ProcessLookupError, ChildProcessError):
            pass
        except OSError as error:
            if error.errno != errno.ESRCH:
                raise

    def close(self) -> None:
        try:
            os.close(self.pidfd)
        except OSError:
            pass


@dataclass
class SupervisorResult:
    process_status: int
    done_status: Optional[int]
    stdout: bytes
    stderr: bytes
    directory: pathlib.Path


def marker_code() -> str:
    return textwrap.dedent(
        """
        import pathlib
        import sys

        def proc_starttime(pid):
            raw = pathlib.Path(f"/proc/{pid}/stat").read_bytes()
            return int(raw[raw.rfind(b")") + 2:].split()[19])

        def mark(path):
            pathlib.Path(path).write_text(
                f"{__import__('os').getpid()} {proc_starttime(__import__('os').getpid())}\\n",
                encoding="ascii",
            )
        """
    )


class SmokeSupervisorTests(unittest.TestCase):
    """Bounded, owned process fixtures for the Linux supervisor contract."""

    def setUp(self) -> None:
        self.temp_directory = tempfile.TemporaryDirectory(prefix="urdia-supervisor-")
        self.directory = pathlib.Path(self.temp_directory.name)
        self._owned: dict[tuple[int, int], OwnedProcess] = {}
        self._popen: list[subprocess.Popen[bytes]] = []
        # Register cleanup after the temporary directory so cleanup runs first
        # (unittest executes addCleanup callbacks in reverse order).
        self.addCleanup(self.temp_directory.cleanup)
        self.addCleanup(self._cleanup_owned_processes)

    def _remember_pid_file(self, path: pathlib.Path) -> None:
        try:
            lines = path.read_text(encoding="ascii").splitlines()
        except (FileNotFoundError, OSError, UnicodeDecodeError):
            return
        for line in lines:
            fields = line.split()
            if len(fields) != 2:
                continue
            try:
                pid, starttime = int(fields[0]), int(fields[1])
            except ValueError:
                continue
            if pid <= 0 or (pid, starttime) in self._owned:
                continue
            pidfd = pidfd_for(pid, starttime)
            if pidfd is not None:
                self._owned[(pid, starttime)] = OwnedProcess(pid, starttime, pidfd)

    def _remember_fixture_processes(self) -> None:
        for path in self.directory.rglob("*.pid"):
            self._remember_pid_file(path)

    def _owned_for_pid(self, pid: int) -> Optional[OwnedProcess]:
        for (candidate, _starttime), ref in self._owned.items():
            if candidate == pid:
                return ref
        return None

    def _remember_descendants_of(self, root_pid: int) -> None:
        """Retain pidfds for an owned process tree snapshot."""

        table: dict[int, tuple[int, int]] = {}
        try:
            entries = pathlib.Path("/proc").iterdir()
        except OSError:
            return
        for entry in entries:
            if not entry.name.isdecimal():
                continue
            record = proc_parent_and_starttime(int(entry.name))
            if record is not None:
                table[int(entry.name)] = record
        pending = [root_pid]
        seen = {root_pid}
        while pending:
            parent = pending.pop()
            for pid, (ppid, starttime) in table.items():
                if ppid != parent or pid in seen:
                    continue
                seen.add(pid)
                pending.append(pid)
                pidfd = pidfd_for(pid, starttime)
                if pidfd is not None:
                    self._owned.setdefault(
                        (pid, starttime), OwnedProcess(pid, starttime, pidfd)
                    )

    def _remember_popen(self, process: subprocess.Popen[bytes]) -> None:
        self._popen.append(process)
        info = proc_info(process.pid)
        if info is None:
            return
        pidfd = pidfd_for(process.pid, info[1])
        if pidfd is not None:
            self._owned[(process.pid, info[1])] = OwnedProcess(process.pid, info[1], pidfd)

    def _wait_for_owned_processes(self, seconds: float) -> None:
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            self._remember_fixture_processes()
            # Reap direct Popen children before checking /proc.  A killed
            # child remains a zombie until its retained Popen is waited on,
            # and a zombie is still visible in /proc.
            for process in self._popen:
                process.poll()
            if not any(ref.same_process_is_present() for ref in self._owned.values()):
                return
            time.sleep(POLL_SECONDS)

    def _cleanup_owned_processes(self) -> None:
        """Terminate every fixture still bearing its recorded identity."""

        self._remember_fixture_processes()
        refs = list(self._owned.values())
        for ref in refs:
            ref.send(signal.SIGTERM)
        self._wait_for_owned_processes(10.0)
        # Marker files can publish descendants after the first scan.  Include
        # every identity discovered during the TERM grace period in the KILL
        # and final ownership checks.
        self._remember_fixture_processes()
        refs = list(self._owned.values())
        for ref in refs:
            ref.send(signal.SIGKILL)
        self._wait_for_owned_processes(5.0)
        self._remember_fixture_processes()
        refs = list(self._owned.values())
        # A marker may have been published during the KILL grace window.  Give
        # any newly observed identity the same pidfd-backed final delivery.
        for ref in refs:
            ref.send(signal.SIGKILL)
        self._wait_for_owned_processes(2.0)
        self._remember_fixture_processes()
        refs = list(self._owned.values())

        survivors = [
            f"{ref.pid}/{ref.starttime}"
            for ref in refs
            if ref.same_process_is_present()
        ]
        for ref in refs:
            ref.close()
        for process in self._popen:
            try:
                process.wait(timeout=1.0)
            except subprocess.TimeoutExpired:
                # The pidfd pass above is authoritative.  A Popen wait here
                # only reaps a child that has already exited.
                pass
            for stream in (process.stdin, process.stdout, process.stderr):
                if stream is not None:
                    stream.close()
        if survivors:
            raise RuntimeError(f"fixture processes survived pidfd cleanup: {survivors}")

    def _run_supervised(
        self,
        fixture_code: str,
        *,
        timeout: float,
        kill_after: float,
        nofile: Optional[int] = None,
    ) -> SupervisorResult:
        stdout_path = self.directory / "command.stdout"
        stderr_path = self.directory / "command.stderr"
        done_path = self.directory / "supervisor.done"
        ready_path = self.directory / "supervisor.ready"
        command = [PYTHON, "-I", "-B", "-c", fixture_code]
        argv = [
            PYTHON,
            "-I",
            "-B",
            str(SUPERVISOR),
            "--timeout",
            str(timeout),
            "--kill-after",
            str(kill_after),
            "--stdout",
            str(stdout_path),
            "--stderr",
            str(stderr_path),
            "--done-file",
            str(done_path),
            "--ready-file",
            str(ready_path),
            "--",
            *command,
        ]
        preexec_fn = None
        if nofile is not None:
            import resource

            def set_nofile() -> None:
                resource.setrlimit(resource.RLIMIT_NOFILE, (nofile, nofile))

            preexec_fn = set_nofile
        process = subprocess.Popen(
            argv,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            close_fds=True,
            preexec_fn=preexec_fn,
        )
        self._remember_popen(process)
        try:
            stdout, stderr = process.communicate(timeout=20.0)
        except subprocess.TimeoutExpired as error:
            self._remember_fixture_processes()
            for ref in list(self._owned.values()):
                ref.send(signal.SIGKILL)
            try:
                stdout, stderr = process.communicate(timeout=5.0)
            except subprocess.TimeoutExpired:
                self.fail("supervisor did not exit after pidfd cleanup")
            self.fail(f"supervisor exceeded the 20 second bound: {error}")
        self._remember_fixture_processes()
        done_status: Optional[int] = None
        try:
            done_status = int(done_path.read_text(encoding="ascii").strip())
        except (FileNotFoundError, OSError, ValueError):
            pass
        return SupervisorResult(process.returncode, done_status, stdout, stderr, self.directory)

    def _assert_no_fixture_processes(self) -> None:
        self._remember_fixture_processes()
        survivors = [
            (ref.pid, ref.starttime)
            for ref in self._owned.values()
            if ref.same_process_is_present()
        ]
        self.assertEqual(survivors, [], f"fixture descendants survived: {survivors}")

    def _read_key_value_marker(self, path: pathlib.Path) -> dict[str, str]:
        values: dict[str, str] = {}
        for line in path.read_text(encoding="utf-8").splitlines():
            key, separator, value = line.partition("=")
            if separator:
                values[key] = value
        return values

    def _run_signal_self_test(self, *, startup: bool) -> None:
        evidence = self.directory / ("signal-startup-evidence" if startup else "signal-evidence")
        evidence.mkdir()
        bash = "/bin/bash" if pathlib.Path("/bin/bash").is_file() else "/usr/bin/bash"
        if not pathlib.Path(bash).is_file():
            self.skipTest("no Bash interpreter is available")
        process = subprocess.Popen(
            [
                bash,
                str(ROOT / "scripts" / "smoke-test.sh"),
                "--signal-self-test",
                "--source-dir",
                str(ROOT),
                "--evidence-dir",
                str(evidence),
            ],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            close_fds=True,
        )
        self._remember_popen(process)
        marker = evidence / ("signal-test-top-ready" if startup else "signal-test-ready")
        started = time.monotonic()
        while not marker.exists():
            if process.poll() is not None:
                try:
                    stdout, stderr = process.communicate(timeout=5.0)
                except subprocess.TimeoutExpired:
                    self.fail("signal smoke exited but its pipes did not close")
                self.fail(f"signal smoke exited before readiness: {stdout!r} {stderr!r}")
            if time.monotonic() - started >= 30.0:
                self.fail("signal smoke did not publish its readiness marker within 30 seconds")
            time.sleep(POLL_SECONDS)

        if startup:
            top_pid = process.pid
            # The top-ready marker is written before the long-running check
            # starts.  Capture any already-created startup descendants with
            # their start times and pidfds before signalling the top process.
            self._remember_descendants_of(top_pid)
            self._remember_fixture_processes()
        else:
            values = self._read_key_value_marker(marker)
            try:
                top_pid = int(values["top_pid"])
            except (KeyError, ValueError) as error:
                self.fail(f"malformed signal marker: {error}")
            self.assertEqual(top_pid, process.pid)
            identity_paths = (
                evidence / "signal-stub-identity.pid",
                evidence / "signal-child-identity.pid",
            )
            for key, identity_path in zip(
                ("stub_pid_file", "child_pid_file"), identity_paths
            ):
                path = pathlib.Path(values[key])
                self.assertTrue(path.is_file(), f"missing owned PID marker {path}")
                self.assertTrue(identity_path.is_file(), f"missing {identity_path}")
                self._remember_pid_file(identity_path)
                try:
                    pid = int(identity_path.read_text(encoding="ascii").split()[0])
                except (OSError, ValueError, IndexError) as error:
                    self.fail(f"malformed owned PID marker {identity_path}: {error}")
                ref = self._owned_for_pid(pid)
                self.assertIsNotNone(ref, f"could not retain pidfd for {identity_path}")
                self.assertTrue(ref.same_process_is_present())

        top = self._owned_for_pid(top_pid)
        self.assertIsNotNone(top, "could not retain a pidfd for the smoke top-level process")
        signalled = time.monotonic()
        top.send(signal.SIGTERM)
        # Exercise the shell's cleanup grace path with a repeated signal while
        # it is still waiting for the supervisor.  The retained pidfd keeps
        # this second delivery tied to the same top-level process.
        time.sleep(0.1)
        top.send(signal.SIGTERM)
        stdout, stderr = process.communicate(timeout=30.0)
        elapsed = time.monotonic() - signalled
        self.assertEqual(process.returncode, 143, f"stdout={stdout!r} stderr={stderr!r}")
        self.assertLess(elapsed, 30.0, "top-level interrupt was not prompt")

        document = evidence / "smoke-evidence.json"
        try:
            parsed = json.loads(document.read_text(encoding="utf-8"))
        except (FileNotFoundError, OSError, json.JSONDecodeError) as error:
            self.fail(f"signal smoke did not publish valid evidence: {error}")
        self.assertEqual(parsed.get("result"), "fail")
        checks = parsed.get("checks")
        self.assertIsInstance(checks, list)
        self.assertEqual([check.get("name") for check in checks], ["interrupted"])
        self._assert_no_fixture_processes()

    def test_signal_to_smoke_top_level_cleans_long_running_descendants(self) -> None:
        self._run_signal_self_test(startup=False)

    def test_signal_during_smoke_startup_is_fail_closed(self) -> None:
        self._run_signal_self_test(startup=True)

    def test_successful_parent_exit_cleans_its_child(self) -> None:
        child_marker = self.directory / "success-child.pid"
        child_code = marker_code() + "import time; mark(sys.argv[1]); time.sleep(60)"
        fixture = marker_code() + textwrap.dedent(
            f"""
            import subprocess
            import sys
            import time

            child = subprocess.Popen([
                sys.executable, "-I", "-B", "-c",
                {child_code!r},
                {str(child_marker)!r},
            ])
            deadline = time.monotonic() + 3
            while not pathlib.Path({str(child_marker)!r}).exists():
                if time.monotonic() >= deadline:
                    raise SystemExit("child marker was not published")
                time.sleep(0.01)
            raise SystemExit(0)
            """
        )
        result = self._run_supervised(fixture, timeout=5.0, kill_after=0.2)
        self.assertEqual(result.process_status, 0, result.stderr.decode(errors="replace"))
        self.assertEqual(result.done_status, 0)
        self._assert_no_fixture_processes()

    def test_timeout_terminates_term_compliant_parent_and_ignoring_child(self) -> None:
        child_marker = self.directory / "timeout-child.pid"
        parent_marker = self.directory / "timeout-parent.pid"
        child_code = (
            marker_code()
            + "import signal; signal.signal(signal.SIGTERM, signal.SIG_IGN); "
            + "import time; mark(sys.argv[1]); time.sleep(60)"
        )
        fixture = marker_code() + textwrap.dedent(
            f"""
            import signal
            import subprocess
            import sys
            import time

            child = subprocess.Popen([
                sys.executable, "-I", "-B", "-c",
                {child_code!r},
                {str(child_marker)!r},
            ])
            deadline = time.monotonic() + 3
            while not pathlib.Path({str(child_marker)!r}).exists():
                if time.monotonic() >= deadline:
                    raise SystemExit("child marker was not published")
                time.sleep(0.01)
            mark({str(parent_marker)!r})
            def raise_exit():
                raise SystemExit(0)

            signal.signal(signal.SIGTERM, lambda _signum, _frame: raise_exit())

            while True:
                time.sleep(1)
            """
        )
        result = self._run_supervised(fixture, timeout=0.25, kill_after=0.15)
        self.assertTrue(child_marker.is_file(), "the TERM-ignoring child never reached readiness")
        self.assertTrue(parent_marker.is_file(), "the TERM-compliant parent never reached readiness")
        self.assertEqual(result.process_status, 124, result.stderr.decode(errors="replace"))
        self.assertEqual(result.done_status, 124)
        self._assert_no_fixture_processes()

    def test_detached_term_ignoring_child_is_cleaned_after_success(self) -> None:
        child_marker = self.directory / "detached-child.pid"
        child_code = (
            marker_code()
            + "import os; os.setsid(); import signal; "
            + "signal.signal(signal.SIGTERM, signal.SIG_IGN); "
            + "import time; mark(sys.argv[1]); time.sleep(60)"
        )
        fixture = marker_code() + textwrap.dedent(
            f"""
            import signal
            import subprocess
            import sys
            import time

            child = subprocess.Popen([
                sys.executable, "-I", "-B", "-c",
                {child_code!r},
                {str(child_marker)!r},
            ])
            deadline = time.monotonic() + 3
            while not pathlib.Path({str(child_marker)!r}).exists():
                if time.monotonic() >= deadline:
                    raise SystemExit("detached child marker was not published")
                time.sleep(0.01)
            raise SystemExit(0)
            """
        )
        result = self._run_supervised(fixture, timeout=5.0, kill_after=0.2)
        self.assertEqual(result.process_status, 0, result.stderr.decode(errors="replace"))
        self.assertEqual(result.done_status, 0)
        self._assert_no_fixture_processes()

    def test_many_short_lived_children_do_not_leak_pidfds_under_low_nofile(self) -> None:
        children_marker = self.directory / "many-children.pid"
        fixture = marker_code() + textwrap.dedent(
            f"""
            import subprocess
            import sys
            import time

            marker = pathlib.Path({str(children_marker)!r})
            marker.write_text("", encoding="ascii")
            for _batch in range(8):
                children = []
                for _ in range(16):
                    child = subprocess.Popen([
                        sys.executable, "-I", "-B", "-c",
                        "import time; time.sleep(0.08)",
                    ])
                    starttime = proc_starttime(child.pid)
                    if starttime is None:
                        raise SystemExit("child disappeared before it could be recorded")
                    with marker.open("a", encoding="ascii") as stream:
                        stream.write(f"{{child.pid}} {{starttime}}\\n")
                    children.append(child)
                for child in children:
                    child.wait()
            mark({str(self.directory / "many-parent.pid")!r})
            """
        )
        result = self._run_supervised(fixture, timeout=10.0, kill_after=0.2, nofile=32)
        self.assertEqual(result.process_status, 0, result.stderr.decode(errors="replace"))
        self.assertEqual(result.done_status, 0)
        self._assert_no_fixture_processes()

    def test_unrelated_owned_non_ascii_comm_process_survives_supervised_command(self) -> None:
        ready = self.directory / "unrelated.ready"
        background_code = textwrap.dedent(
            f"""
            import ctypes
            import pathlib
            import time

            libc = ctypes.CDLL(None, use_errno=True)
            libc.prctl.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
            libc.prctl.restype = ctypes.c_int
            if libc.prctl(15, "owned-π)".encode("utf-8"), 0, 0, 0) != 0:
                raise OSError(ctypes.get_errno(), "prctl(PR_SET_NAME) failed")
            pathlib.Path({str(ready)!r}).write_text("ready\\n", encoding="ascii")
            while True:
                time.sleep(60)
            """
        )
        background = subprocess.Popen(
            [PYTHON, "-I", "-B", "-c", background_code],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            close_fds=True,
        )
        self._remember_popen(background)
        deadline = time.monotonic() + 3
        while not ready.exists():
            if background.poll() is not None:
                stderr = background.stderr.read() if background.stderr else b""
                self.fail(f"background fixture exited: {stderr!r}")
            if time.monotonic() >= deadline:
                self.fail("background fixture did not publish its ready marker")
            time.sleep(POLL_SECONDS)

        fixture = "import time; time.sleep(0.15)"
        result = self._run_supervised(fixture, timeout=5.0, kill_after=0.2)
        self.assertEqual(result.process_status, 0, result.stderr.decode(errors="replace"))
        self.assertEqual(result.done_status, 0)
        self.assertIsNone(
            background.poll(),
            "the unrelated process must remain alive until fixture cleanup",
        )
        info = proc_info(background.pid)
        self.assertIsNotNone(info, "the unrelated process disappeared unexpectedly")
        self.assertIn("π".encode("utf-8"), proc_comm(background.pid) or b"")


if __name__ == "__main__":
    unittest.main(verbosity=2)
