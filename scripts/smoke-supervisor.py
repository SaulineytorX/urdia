#!/usr/bin/env python3
"""Bound one command and clean up every descendant it leaves behind.

The release smoke test runs this small Linux-only supervisor instead of relying
on ``timeout`` to find descendants after their parent has exited.  The
supervisor is a child subreaper, starts the command in a fresh session, keeps
the command leader as a non-reaped zombie while it tears down the session, and
uses pidfds for descendants that leave the original process group.
"""

from __future__ import annotations

import argparse
import ctypes
import errno
import os
import platform
import signal
import subprocess
import sys
import time
from dataclasses import dataclass
from typing import Optional


PR_SET_CHILD_SUBREAPER = 36
POLL_SECONDS = 0.02

_libc = ctypes.CDLL(None, use_errno=True)
_libc.prctl.restype = ctypes.c_int


class SupervisorError(RuntimeError):
    """A prerequisite or process-supervision operation failed."""


@dataclass
class ProcInfo:
    state: bytes
    ppid: int
    pgrp: int
    session: int
    starttime: int


@dataclass
class ProcessRef:
    pid: int
    starttime: int
    pidfd: Optional[int]
    reaped: bool = False
    gone: bool = False


def set_child_subreaper() -> None:
    if platform.system() != "Linux":
        raise SupervisorError("the smoke supervisor requires Linux")
    result = _libc.prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0)
    if result != 0:
        error = ctypes.get_errno()
        raise SupervisorError(
            f"prctl(PR_SET_CHILD_SUBREAPER) failed: {os.strerror(error)}"
        )


def read_proc_info(pid: int) -> Optional[ProcInfo]:
    """Read the stable identity and parentage fields from /proc/<pid>/stat."""

    try:
        with open(f"/proc/{pid}/stat", "rb") as stream:
            text = stream.read()
    except (FileNotFoundError, PermissionError, ProcessLookupError):
        return None
    closing_paren = text.rfind(b")")
    if closing_paren < 0:
        return None
    fields = text[closing_paren + 2 :].split()
    # After the command name, fields[0] is stat field 3.  starttime is
    # stat field 22, therefore index 19 in this slice.
    if len(fields) < 20:
        return None
    try:
        return ProcInfo(
            state=fields[0],
            ppid=int(fields[1]),
            pgrp=int(fields[2]),
            session=int(fields[3]),
            starttime=int(fields[19]),
        )
    except ValueError:
        return None


def read_proc_table() -> dict[int, ProcInfo]:
    table: dict[int, ProcInfo] = {}
    try:
        names = os.listdir("/proc")
    except OSError as error:
        raise SupervisorError(f"could not inspect /proc: {error}") from error
    for name in names:
        if not name.isdecimal():
            continue
        info = read_proc_info(int(name))
        if info is not None:
            table[int(name)] = info
    return table


def open_pidfd(pid: int) -> Optional[int]:
    try:
        return os.pidfd_open(pid, 0)
    except AttributeError as error:
        raise SupervisorError("Python does not expose pidfd_open") from error
    except OSError as error:
        if error.errno == errno.ESRCH:
            return None
        raise


def send_pidfd(pidfd: int, signum: int) -> None:
    try:
        signal.pidfd_send_signal(pidfd, signum)
    except ProcessLookupError:
        pass
    except OSError as error:
        if error.errno not in (errno.ESRCH, errno.ECHILD):
            raise


def waitid_flags(*, nohang: bool, nowait: bool = False) -> int:
    flags = os.WEXITED
    if nohang:
        flags |= os.WNOHANG
    if nowait:
        flags |= os.WNOWAIT
    return flags


def status_from_waitid(info: os.waitid_result) -> int:
    if info.si_code == getattr(os, "CLD_EXITED", 1):
        return int(info.si_status)
    return 128 + int(info.si_status)


class Supervisor:
    def __init__(
        self,
        timeout: float,
        kill_after: float,
        command: list[str],
        interrupt_file: Optional[str],
    ):
        self.timeout = timeout
        self.kill_after = kill_after
        self.command = command
        self.interrupt_file = interrupt_file
        self.process: Optional[subprocess.Popen[bytes]] = None
        self.leader: Optional[ProcessRef] = None
        self.leader_status: Optional[int] = None
        self.leader_pgid: Optional[int] = None
        self.descendants: dict[int, ProcessRef] = {}
        self.interrupted: Optional[int] = None
        self._handler_installed = False

    def cancel_requested(self) -> bool:
        return bool(self.interrupt_file and os.path.exists(self.interrupt_file))

    def install_handlers(self) -> None:
        def handle(signum: int, _frame: object) -> None:
            # Do not perform process operations from a Python signal handler.
            # The main loop observes this flag and keeps the supervisor alive
            # until its complete cleanup path has finished.
            if self.interrupted is None:
                self.interrupted = signum

        for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(signum, handle)
        # Retain exited children for explicit waitid calls even if the caller
        # launched us with SIGCHLD ignored.
        signal.signal(signal.SIGCHLD, signal.SIG_DFL)
        self._handler_installed = True

    def spawn(self, stdout_path: str, stderr_path: str) -> None:
        try:
            stdout = open(stdout_path, "wb")
            stderr = open(stderr_path, "wb")
        except OSError as error:
            raise SupervisorError(f"could not open command logs: {error}") from error
        try:
            process = subprocess.Popen(
                self.command,
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                close_fds=True,
                start_new_session=True,
            )
        except OSError:
            stdout.close()
            stderr.close()
            raise
        finally:
            # The child has its own descriptor references after Popen returns.
            stdout.close()
            stderr.close()
        # Retain the Popen object until the explicit waitid reap.  Letting its
        # temporary local be destroyed would call _internal_poll and could reap
        # a very short-lived leader before WNOWAIT reserves it.
        self.process = process
        starttime = self._require_starttime(process.pid)
        self.leader = ProcessRef(process.pid, starttime, None)
        self.leader_pgid = process.pid
        pidfd = open_pidfd(process.pid)
        if pidfd is None:
            raise SupervisorError("pidfd_open is required for safe descendant cleanup")
        self.leader.pidfd = pidfd

    @staticmethod
    def _require_starttime(pid: int) -> int:
        info = read_proc_info(pid)
        if info is None:
            raise SupervisorError(f"the monitored process {pid} disappeared at startup")
        return info.starttime

    def inspect_leader(self, *, nowait: bool) -> Optional[int]:
        if self.leader is None or self.leader.reaped:
            return self.leader_status
        try:
            result = os.waitid(
                os.P_PIDFD,
                self.leader.pidfd,
                waitid_flags(nohang=True, nowait=nowait),
            )
        except ChildProcessError as error:
            raise SupervisorError("the supervisor lost the monitored leader") from error
        if result is not None and result.si_pid == self.leader.pid:
            self.leader_status = status_from_waitid(result)
        return self.leader_status

    def collect_descendants(self, table: Optional[dict[int, ProcInfo]] = None) -> None:
        if self.leader is None:
            return
        self.prune_descendants()
        if table is None:
            table = read_proc_table()
        children: dict[int, list[int]] = {}
        for pid, info in table.items():
            children.setdefault(info.ppid, []).append(pid)
        roots = {os.getpid(), self.leader.pid}
        seen: set[int] = set()
        pending = list(roots)
        while pending:
            parent = pending.pop()
            for child in children.get(parent, []):
                if child in seen or child in roots:
                    continue
                seen.add(child)
                pending.append(child)
        for pid in seen:
            info = table.get(pid)
            if info is None or pid == self.leader.pid:
                continue
            existing = self.descendants.get(pid)
            if existing is not None and existing.starttime == info.starttime:
                continue
            if existing is not None:
                self.close_ref(existing)
            pidfd = open_pidfd(pid)
            if pidfd is None:
                # A process that disappeared between /proc and pidfd_open is
                # already gone.  Refusing to signal a raw, potentially reused
                # PID is the safe outcome.
                continue
            current = read_proc_info(pid)
            if current is None or current.starttime != info.starttime:
                # The numeric PID changed while it was being inspected.  Do
                # not retain or signal a ref for the previous process.
                try:
                    os.close(pidfd)
                except OSError:
                    pass
                continue
            self.descendants[pid] = ProcessRef(pid, info.starttime, pidfd)

    @staticmethod
    def close_ref(ref: ProcessRef) -> None:
        if ref.pidfd is not None:
            try:
                os.close(ref.pidfd)
            except OSError:
                pass
            ref.pidfd = None

    def ref_info(self, ref: ProcessRef) -> Optional[ProcInfo]:
        info = read_proc_info(ref.pid)
        if info is None or info.starttime != ref.starttime:
            ref.gone = True
            return None
        return info

    def reap_ref(self, ref: ProcessRef) -> None:
        if ref.reaped or ref.gone or ref.pidfd is None:
            return
        try:
            result = os.waitid(
                os.P_PIDFD,
                ref.pidfd,
                waitid_flags(nohang=True),
            )
        except ChildProcessError:
            # The original parent still owns this process.  It may become an
            # adopted child on the next scan after that parent exits.
            return
        except OSError as error:
            if error.errno in (errno.ESRCH, errno.ECHILD):
                ref.gone = True
                return
            raise
        if result is not None:
            ref.reaped = True
            ref.gone = True

    def reap_descendants(self) -> None:
        for ref in list(self.descendants.values()):
            self.ref_info(ref)
            self.reap_ref(ref)
        self.prune_descendants()

    def prune_descendants(self) -> None:
        for pid, ref in list(self.descendants.items()):
            if ref.reaped or ref.gone:
                self.close_ref(ref)
                del self.descendants[pid]

    def leader_group_is_valid(self) -> bool:
        if self.leader is None or self.leader_pgid is None:
            return False
        info = self.ref_info(self.leader)
        return info is not None and info.pgrp == self.leader_pgid

    def signal_group(self, signum: int) -> None:
        # The leader remains a WNOWAIT zombie until this function is no longer
        # needed.  That reserves its PID/PGID and prevents a blind signal from
        # reaching a reused process group.
        if not self.leader_group_is_valid():
            return
        try:
            os.killpg(self.leader_pgid, signum)
        except ProcessLookupError:
            pass
        except PermissionError as error:
            raise SupervisorError(f"could not signal the monitored process group: {error}") from error

    def signal_descendants(self, signum: int) -> None:
        for ref in list(self.descendants.values()):
            if ref.reaped or ref.gone or ref.pidfd is None:
                continue
            if self.ref_info(ref) is None:
                continue
            send_pidfd(ref.pidfd, signum)

    def has_live_descendants(self) -> bool:
        for ref in self.descendants.values():
            if ref.reaped or ref.gone:
                continue
            info = self.ref_info(ref)
            if info is not None:
                return True
        return False

    def wait_for_cleanup(self, seconds: float, signum: int) -> bool:
        deadline = time.monotonic() + max(0.0, seconds)
        while True:
            self.collect_descendants()
            self.signal_group(signum)
            self.signal_descendants(signum)
            self.reap_descendants()
            leader_done = self.inspect_leader(nowait=True) is not None
            if leader_done and not self.has_live_descendants():
                return True
            if time.monotonic() >= deadline:
                return False
            time.sleep(POLL_SECONDS)

    def cleanup(self) -> None:
        # A TERM grace period lets cooperative commands finish.  KILL then
        # reaches both the original session and descendants that called setsid.
        complete = self.wait_for_cleanup(self.kill_after, signal.SIGTERM)
        if not complete:
            complete = self.wait_for_cleanup(max(self.kill_after, 1.0), signal.SIGKILL)
        # Keep collecting adopted descendants until every observed process has
        # either exited and been reaped or has disappeared.  SIGKILL cannot be
        # ignored, so this loop is bounded in practice while still allowing
        # subreaper adoption and zombie reaping to settle.
        deadline = time.monotonic() + max(5.0, self.kill_after * 5.0)
        while not complete and time.monotonic() < deadline:
            self.collect_descendants()
            self.signal_group(signal.SIGKILL)
            self.signal_descendants(signal.SIGKILL)
            self.reap_descendants()
            complete = (
                self.inspect_leader(nowait=True) is not None
                and not self.has_live_descendants()
            )
            if not complete:
                time.sleep(POLL_SECONDS)
        self.collect_descendants()
        self.reap_descendants()
        complete = (
            self.inspect_leader(nowait=True) is not None
            and not self.has_live_descendants()
        )
        if not complete:
            raise SupervisorError("the monitored command or a descendant survived cleanup")

    def reap_leader(self) -> int:
        if self.leader is None or self.leader.pidfd is None:
            raise SupervisorError("the monitored leader was not started")
        if self.inspect_leader(nowait=True) is None:
            raise SupervisorError("the monitored leader was not stopped before reap")
        try:
            result = os.waitid(
                os.P_PIDFD,
                self.leader.pidfd,
                waitid_flags(nohang=False),
            )
        except ChildProcessError as error:
            raise SupervisorError("the monitored leader was reaped before cleanup completed") from error
        self.leader.reaped = True
        if self.leader_status is None and result is not None:
            self.leader_status = status_from_waitid(result)
        if self.leader_status is None:
            raise SupervisorError("the monitored leader exited without a status")
        if self.process is not None:
            self.process.returncode = self.leader_status
        return self.leader_status

    def close(self) -> None:
        if self.leader is not None:
            self.close_ref(self.leader)
        for ref in self.descendants.values():
            self.close_ref(ref)

    def run(self, stdout_path: str, stderr_path: str, ready_path: Optional[str]) -> int:
        set_child_subreaper()
        self.install_handlers()
        write_marker(ready_path)
        if self.interrupted is not None or self.cancel_requested():
            if self.interrupted is None:
                self.interrupted = signal.SIGTERM
            return 128 + self.interrupted
        try:
            self.spawn(stdout_path, stderr_path)
            deadline = time.monotonic() + max(0.0, self.timeout)
            timed_out = False
            while True:
                self.collect_descendants()
                self.reap_descendants()
                if self.interrupted is not None or self.cancel_requested():
                    if self.interrupted is None:
                        self.interrupted = signal.SIGTERM
                    break
                if self.inspect_leader(nowait=True) is not None:
                    break
                if time.monotonic() >= deadline:
                    timed_out = True
                    break
                time.sleep(POLL_SECONDS)
            if self.interrupted is not None:
                self.cleanup()
                self.reap_leader()
                return 128 + self.interrupted
            if timed_out:
                self.cleanup()
                self.reap_leader()
                return 124
            self.cleanup()
            return self.reap_leader()
        except BaseException:
            # Once Popen has succeeded, every exceptional path owns the same
            # cleanup obligation as a timeout.  Keep the leader reserved while
            # making a best effort to terminate and reap it before reporting a
            # supervisor failure.
            # Signal first, before another /proc scan or pidfd allocation can
            # repeat the failure. Popen is retained and never polls the leader;
            # its PID stays reserved until our explicit reap.
            leader_unreaped = self.leader is None or not self.leader.reaped
            if self.process is not None and leader_unreaped:
                try:
                    os.killpg(self.process.pid, signal.SIGKILL)
                except OSError:
                    pass
                for ref in self.descendants.values():
                    if ref.pidfd is not None:
                        try:
                            send_pidfd(ref.pidfd, signal.SIGKILL)
                        except OSError:
                            pass
            if self.leader is not None and self.leader.pidfd is not None and leader_unreaped:
                try:
                    self.cleanup()
                except BaseException:
                    pass
                if self.leader_status is not None:
                    try:
                        self.reap_leader()
                    except BaseException:
                        pass
            elif self.process is not None and leader_unreaped:
                # Startup can fail before a leader ref exists. No more group
                # signals follow this wait, and it must not block indefinitely.
                try:
                    self.process.wait(timeout=max(1.0, self.kill_after))
                except subprocess.TimeoutExpired:
                    pass
            raise
        finally:
            self.close()


def write_done(path: Optional[str], status: int) -> None:
    if not path:
        return
    temporary = f"{path}.tmp.{os.getpid()}"
    try:
        with open(temporary, "w", encoding="ascii") as stream:
            stream.write(f"{status}\n")
        os.replace(temporary, path)
    except OSError:
        try:
            os.unlink(temporary)
        except OSError:
            pass


def write_marker(path: Optional[str]) -> None:
    if not path:
        return
    temporary = f"{path}.tmp.{os.getpid()}"
    try:
        with open(temporary, "w", encoding="ascii") as stream:
            stream.write(f"{os.getpid()}\n")
        os.replace(temporary, path)
    except OSError:
        try:
            os.unlink(temporary)
        except OSError:
            pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--timeout", type=float)
    parser.add_argument("--kill-after", type=float)
    parser.add_argument("--stdout")
    parser.add_argument("--stderr")
    parser.add_argument("--done-file")
    parser.add_argument("--ready-file")
    parser.add_argument("--interrupt-file")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.check:
        return args
    if args.command[:1] == ["--"]:
        args.command = args.command[1:]
    if not args.command or args.timeout is None or args.kill_after is None:
        parser.error("--timeout, --kill-after, --stdout, --stderr, and a command are required")
    if not args.stdout or not args.stderr:
        parser.error("--stdout and --stderr are required")
    if args.timeout < 0 or args.kill_after < 0:
        parser.error("timeout values must be non-negative")
    return args


def check_prerequisites() -> None:
    set_child_subreaper()
    required = ("pidfd_open", "waitid", "P_PIDFD", "WNOWAIT")
    missing = [name for name in required if not hasattr(os, name)]
    if not hasattr(signal, "pidfd_send_signal"):
        missing.append("signal.pidfd_send_signal")
    if missing:
        raise SupervisorError("Python is missing Linux process APIs: " + ", ".join(missing))
    pidfd = open_pidfd(os.getpid())
    if pidfd is None:
        raise SupervisorError("pidfd_open is unavailable")
    os.close(pidfd)
    info = read_proc_info(os.getpid())
    if info is None:
        raise SupervisorError("/proc process inspection is unavailable")


def main() -> int:
    pending_signal: list[Optional[int]] = [None]

    def remember_signal(signum: int, _frame: object) -> None:
        if pending_signal[0] is None:
            pending_signal[0] = signum

    for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(signum, remember_signal)
    args = parse_args()
    try:
        if args.check:
            check_prerequisites()
            return 0
        supervisor = Supervisor(
            args.timeout,
            args.kill_after,
            args.command,
            args.interrupt_file,
        )
        supervisor.interrupted = pending_signal[0]
        status = supervisor.run(args.stdout, args.stderr, args.ready_file)
    except (OSError, SupervisorError, ValueError) as error:
        print(f"smoke supervisor: {error}", file=sys.stderr)
        status = 125
    write_done(args.done_file, status)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
