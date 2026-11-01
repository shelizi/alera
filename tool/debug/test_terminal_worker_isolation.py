"""Windows PTY Worker fault isolation and Host reconnect integration smoke.

Run against an already-built sidecar:
  python tool/debug/test_terminal_worker_isolation.py D:/c/t/debug/alera.exe

The worker listens on loopback only; the test's runtime directory and registry
are isolated under the current worktree. No live user terminals are touched.
"""

import ctypes
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time


def write_frame(stream, kind, payload):
    assert len(payload) <= 4 * 1024 * 1024
    wire = kind + len(payload).to_bytes(4, "little") + payload
    if hasattr(stream, "sendall"):
        stream.sendall(wire)
    else:
        stream.write(wire)
        stream.flush()


def receive_exact(sock, size):
    data = bytearray()
    while len(data) < size:
        chunk = sock.recv(size - len(data))
        if not chunk:
            raise EOFError("worker closed during reconnect authentication")
        data.extend(chunk)
    return bytes(data)


def open_shell_wait_handle(pid):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
    kernel.OpenProcess.restype = ctypes.c_void_p
    kernel.WaitForSingleObject.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
    kernel.WaitForSingleObject.restype = ctypes.c_uint32
    kernel.CloseHandle.argtypes = [ctypes.c_void_p]
    kernel.CloseHandle.restype = ctypes.c_int
    handle = kernel.OpenProcess(0x00100000, False, pid)
    if not handle:
        raise AssertionError(f"cannot open shell PID {pid}: {ctypes.get_last_error()}")
    return kernel, handle


class Worker:
    def __init__(self, executable, runtime, label, *, worker_env=None, max_backlog_bytes=1024 * 1024):
        self.label = label
        self.session_id = f"isolation-smoke-{label}"
        self.data = bytearray()
        self.lock = threading.Condition()
        self.runtime_dir = runtime
        worker_environment = dict(os.environ)
        if worker_env:
            worker_environment.update(worker_env)
        self.process = subprocess.Popen(
            [str(executable), "__alera-terminal-pty-worker"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            env=worker_environment,
        )
        try:
            request = {
                "sessionId": self.session_id,
                "workspaceId": "isolated-test-workspace",
                "tabId": f"isolated-test-{label}",
                "workingDirectory": str(Path.cwd()),
                "runtimeDir": str(runtime),
                "cols": 80,
                "rows": 24,
                "initialOutputStreamBytes": 0,
                "maxBacklogBytes": max_backlog_bytes,
                "launch": {
                    "label": "cmd",
                    "shell": os.environ.get("ComSpec", "cmd.exe"),
                    "arguments": ["/Q", "/K"],
                    "environment": {**worker_environment, "ALERA_TEST_PART": "WORKER"},
                },
            }
            write_frame(self.process.stdin, b"I", json.dumps(request).encode())
            ready = self.process.stdout.read(5)
            if len(ready) != 5 or ready[0:1] != b"R":
                raise AssertionError(f"{label}: failed to start worker, got {ready!r}")
            self.shell_pid = int.from_bytes(ready[1:], "little")
            assert self.shell_pid > 0
            self.process.stdin.close()
            self.process.stdout.close()
            self.attach()
        except BaseException:
            # Constructors that fail before being appended to the test list
            # must still kill/reap their Worker and avoid orphan Shell jobs.
            if self.process.poll() is None:
                self.process.kill()
            self.process.wait(timeout=10)
            for pipe in (self.process.stdin, self.process.stdout, self.process.stderr):
                if pipe is not None and not pipe.closed:
                    pipe.close()
            if hasattr(self, "socket"):
                self.socket.close()
            raise

    def registry(self):
        digest = hashlib.sha256(self.session_id.encode()).hexdigest()
        path = Path(self.runtime_dir) / "pty-workers" / f"{digest}.json"
        return json.loads(path.read_text(encoding="utf-8"))

    def attach(self):
        record = self.registry()
        assert record["workerPid"] == self.process.pid, "worker PID changed"
        assert record["shellPid"] == self.shell_pid, "shell PID changed"
        sock = socket.create_connection(("127.0.0.1", record["port"]), timeout=5)
        sock.settimeout(5)
        auth = {
            "sessionId": self.session_id,
            "token": record["token"],
            "resumeCursor": 0,
        }
        write_frame(sock, b"A", json.dumps(auth).encode())
        ack = receive_exact(sock, 13)
        assert ack[0:1] == b"R", f"{self.label}: bad reconnect marker {ack!r}"
        assert int.from_bytes(ack[1:5], "little") == self.shell_pid
        self.replay_start_cursor = int.from_bytes(ack[5:13], "little")
        self.socket = sock
        with self.lock:
            self.data.clear()
        self.reader = threading.Thread(target=self.read_loop, daemon=True)
        self.reader.start()

    def read_loop(self):
        try:
            self.socket.settimeout(None)
            while True:
                chunk = self.socket.recv(8192)
                if not chunk:
                    break
                with self.lock:
                    self.data.extend(chunk)
                    self.lock.notify_all()
        except (OSError, ValueError):
            pass
        finally:
            with self.lock:
                self.lock.notify_all()

    def read_until(self, predicate, timeout):
        deadline = time.monotonic() + timeout
        with self.lock:
            while not predicate(self.data):
                if self.process.poll() is not None:
                    raise AssertionError(
                        f"{self.label}: worker died with code {self.process.returncode}"
                    )
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError(
                        f"{self.label}: expected output missing, last={bytes(self.data[-1500:])!r}"
                    )
                self.lock.wait(min(0.2, remaining))
            return bytes(self.data)

    def send_bytes(self, payload):
        write_frame(self.socket, b"W", payload)

    def assert_echo(self, expression, expected):
        self.send_bytes(f"echo {expression}\r".encode())
        self.read_until(lambda data: b"\x1b[6n" in data or expected.encode() in data, 10)
        # Windows ConPTY requests the initial cursor position. In Alera the UI
        # responds; the smoke driver must emulate the same behavior.
        if b"\x1b[6n" in self.data:
            self.send_bytes(b"\x1b[1;1R")
        self.read_until(lambda data: expected.encode() in data, 15)

    def disconnect(self):
        try:
            self.socket.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.socket.close()
        self.reader.join(timeout=3)

    def close(self):
        try:
            self.disconnect()
        except (OSError, AttributeError):
            pass
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=10)
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            if stream and not stream.closed:
                stream.close()


def main():
    if sys.platform != "win32":
        raise SystemExit("Windows only")
    if len(sys.argv) != 2:
        raise SystemExit("usage: test_terminal_worker_isolation.py PATH_TO_ALERA_EXE")
    executable = Path(sys.argv[1]).resolve()
    if not executable.is_file():
        raise SystemExit(f"executable not found: {executable}")

    with tempfile.TemporaryDirectory(prefix="alera-worker-smoke-") as runtime:
        workers = []
        try:
            first = Worker(executable, runtime, "first")
            workers.append(first)
            second = Worker(executable, runtime, "second")
            workers.append(second)
            first.assert_echo("ALERA_%ALERA_TEST_PART%_FIRST", "ALERA_WORKER_FIRST")
            second.assert_echo("ALERA_%ALERA_TEST_PART%_SECOND", "ALERA_WORKER_SECOND")

            # A loopback process without the registry token must not be able
            # to replace the authenticated Host or send PTY input.
            endpoint = second.registry()
            with socket.create_connection(("127.0.0.1", endpoint["port"]), timeout=5) as untrusted:
                untrusted.settimeout(3)
                write_frame(untrusted, b"A", json.dumps({
                    "sessionId": second.session_id,
                    "token": "0" * 36,
                    "resumeCursor": 0,
                }).encode())
                try:
                    response = untrusted.recv(1)
                except (ConnectionAbortedError, ConnectionResetError):
                    response = b""
                assert response == b"", "worker accepted incorrect reconnect token"
            second.assert_echo("ALERA_%ALERA_TEST_PART%_AUTH_OK", "ALERA_WORKER_AUTH_OK")

            # Verify killed worker's shell Job exits, not just its supervisor.
            kernel, handle = open_shell_wait_handle(first.shell_pid)
            try:
                first.process.kill()
                first.process.wait(timeout=10)
                assert kernel.WaitForSingleObject(handle, 10000) == 0, (
                    "PTY worker crashed but its shell process survived"
                )
            finally:
                kernel.CloseHandle(handle)
            second.assert_echo(
                "ALERA_%ALERA_TEST_PART%_AFTER_PEER_CRASH",
                "ALERA_WORKER_AFTER_PEER_CRASH",
            )

            # Simulate old Host losing its transport while worker stays alive;
            # reconnect from a new socket and reuse the same OS worker/shell.
            original_worker_pid = second.process.pid
            original_shell_pid = second.shell_pid
            second.send_bytes(
                b"ping -n 3 127.0.0.1 >NUL & echo ALERA_OFFLINE_REPLAY\r"
            )
            second.disconnect()
            assert second.process.poll() is None, "Worker died with original Host transport"
            time.sleep(3)
            second.attach()
            second.read_until(
                lambda data: b"ALERA_OFFLINE_REPLAY" in data, 15
            )
            assert second.process.pid == original_worker_pid
            assert second.shell_pid == original_shell_pid
            second.assert_echo(
                "ALERA_%ALERA_TEST_PART%_RECONNECTED",
                "ALERA_WORKER_RECONNECTED",
            )

            # Normal shell termination must close its Worker and preserve output.
            second.send_bytes(b"echo WORKER_NORMAL_EXIT\r")
            second.read_until(lambda data: b"WORKER_NORMAL_EXIT" in data, 10)
            second.send_bytes(b"exit\r")
            second.process.wait(timeout=12)
            assert second.process.returncode == 0, "worker did not exit with shell"
            print("PASS: worker crash isolated; shell Job terminated; Host socket reconnect preserved session")
        finally:
            for worker in reversed(workers):
                worker.close()


if __name__ == "__main__":
    main()
