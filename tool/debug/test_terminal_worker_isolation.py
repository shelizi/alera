"""Windows smoke test: one worker dies, a separate PTY keeps working.

Usage: python tool/debug/test_terminal_worker_isolation.py D:/c/t/debug/alera.exe
Build the alera-cli executable first; never use a Cargo unit-test harness as a worker.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time


def write_frame(stream, kind, payload):
    assert len(payload) <= 4 * 1024 * 1024
    stream.write(kind + len(payload).to_bytes(4, "little") + payload)
    stream.flush()


class Worker:
    def __init__(self, executable, label, arguments=None):
        self.process = subprocess.Popen(
            [str(executable), "__alera-terminal-pty-worker"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
        self.label = label
        self.data = bytearray()
        self.lock = threading.Condition()
        self.reader = threading.Thread(target=self.read_loop, daemon=True)
        self.reader.start()
        launch = {
            "sessionId": f"isolation-smoke-{label}",
            "workingDirectory": str(Path.cwd()),
            "cols": 80,
            "rows": 24,
            "launch": {
                "label": "cmd",
                "shell": os.environ.get("ComSpec", "cmd.exe"),
                "arguments": arguments or ["/Q", "/K"],
                "environment": {**dict(os.environ), "ALERA_TEST_PART": "WORKER"},
            },
        }
        write_frame(self.process.stdin, b"I", json.dumps(launch).encode())
        ready = self.read_until(lambda data: len(data) >= 5, timeout=20)
        assert ready[0:1] == b"R", f"{label}: invalid startup response"
        self.shell_pid = int.from_bytes(ready[1:5], "little")
        assert self.shell_pid > 0, f"{label}: missing shell PID"
        with self.lock:
            del self.data[:5]

    def read_loop(self):
        try:
            while True:
                chunk = self.process.stdout.read1(8192)
                if not chunk:
                    break
                with self.lock:
                    self.data.extend(chunk)
                    self.lock.notify_all()
        finally:
            with self.lock:
                self.lock.notify_all()

    def read_until(self, predicate, timeout):
        deadline = time.monotonic() + timeout
        with self.lock:
            while not predicate(self.data):
                if self.process.poll() is not None:
                    raise AssertionError(f"{self.label}: worker exited unexpectedly: {self.process.returncode}")
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError(f"{self.label}: output not received; received={bytes(self.data[-2048:])!r}; worker={self.process.pid}; shell={getattr(self, 'shell_pid', None)}")
                self.lock.wait(min(0.2, remaining))
            return bytes(self.data)

    def send(self, message):
        write_frame(self.process.stdin, b"W", message.encode())

    def assert_echo(self, expression, expected):
        self.send(f"echo {expression}\r")
        # ConPTY startup asks the emulator for its cursor position.
        # The production UI responds; emulate the reply in this smoke test.
        if b"\x1b[6n" in self.data:
            write_frame(self.process.stdin, b"W", b"\x1b[1;1R")
        self.read_until(lambda data: expected.encode() in data, timeout=15)

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=10)
        self.process.stdin.close()
        self.process.stdout.close()
        if self.process.stderr is not None:
            self.process.stderr.close()


def main():
    if sys.platform != "win32":
        raise SystemExit("Windows only")
    if len(sys.argv) != 2:
        raise SystemExit("usage: test_terminal_worker_isolation.py PATH_TO_ALERA_EXE")
    exe = Path(sys.argv[1]).resolve()
    if not exe.is_file():
        raise SystemExit(f"executable not found: {exe}")
    workers = []
    try:
        first = Worker(exe, "first")
        workers.append(first)
        second = Worker(exe, "second")
        workers.append(second)
        assert first.process.pid != second.process.pid
        first.assert_echo("ALERA_%ALERA_TEST_PART%_FIRST", "ALERA_WORKER_FIRST")
        second.assert_echo("ALERA_%ALERA_TEST_PART%_SECOND", "ALERA_WORKER_SECOND")
        first.process.kill()
        first.process.wait(timeout=10)
        second.assert_echo("ALERA_%ALERA_TEST_PART%_SURVIVED_FIRST_CRASH", "ALERA_WORKER_SURVIVED_FIRST_CRASH")
        third = Worker(exe, "natural-exit", ["/Q", "/C", "echo WORKER_NORMAL_EXIT"])
        workers.append(third)
        third.read_until(lambda data: b"\x1b[6n" in data or b"WORKER_NORMAL_EXIT" in data, timeout=10)
        if b"\x1b[6n" in third.data:
            write_frame(third.process.stdin, b"W", b"\x1b[1;1R")
        third.process.wait(timeout=12)
        assert third.process.returncode == 0, "worker did not exit cleanly"
        third.reader.join(timeout=2)
        assert b"WORKER_NORMAL_EXIT" in third.data, "normal shell output lost"
        second.assert_echo("ALERA_%ALERA_TEST_PART%_AFTER_NORMAL_EXIT", "ALERA_WORKER_AFTER_NORMAL_EXIT")
        print(f"PASS: worker crash isolated; natural exit output preserved")
    finally:
        for worker in reversed(workers):
            worker.close()


if __name__ == "__main__":
    main()
