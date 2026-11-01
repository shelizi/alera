"""Native Unix PTY Worker crash isolation and Host-socket recovery smoke.

Runs on Linux and macOS after building the Rust alera-cli sidecar:
  python3 tool/debug/test_terminal_worker_unix.py rust/target/debug/alera

This exercises the real Rust worker and PTY, not a fake/mock. A temporary
runtime is used and every Worker is terminated and reaped before exit.
"""
import hashlib
import json
import os
from pathlib import Path
import shlex
import signal
import socket
import subprocess
import sys
import tempfile
import time

START_TIMEOUT = 20
COMMAND_TIMEOUT = 15


def send_frame(stream, kind, payload):
    packet = kind + len(payload).to_bytes(4, "little") + payload
    if hasattr(stream, "sendall"):
        stream.sendall(packet)
    else:
        stream.write(packet)
        stream.flush()


def receive_exact(stream, length):
    result = bytearray()
    while len(result) < length:
        chunk = stream.recv(length - len(result))
        if not chunk:
            raise EOFError("PTY worker closed the authenticated socket")
        result.extend(chunk)
    return bytes(result)


def await_file(path, text):
    until = time.monotonic() + COMMAND_TIMEOUT
    while time.monotonic() < until:
        try:
            if path.read_text(encoding="utf-8").strip() == text:
                return
        except (FileNotFoundError, PermissionError):
            pass
        time.sleep(0.04)
    raise AssertionError(f"PTY shell never executed {text}")


class Worker:
    def __init__(self, binary, runtime, label):
        self.label = label
        self.session = "unix-worker-" + label
        self.runtime = Path(runtime)
        self.command = subprocess.Popen(
            [str(binary), "__alera-terminal-pty-worker"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        self.socket = None
        try:
            env = {**os.environ, "TERM": "xterm-256color"}
            request = {
                "sessionId": self.session,
                "workspaceId": "unix-isolation-" + label,
                "tabId": "terminal-tab-" + label,
                "workingDirectory": str(self.runtime),
                "runtimeDir": str(self.runtime),
                "launch": {
                    "label": "sh",
                    "shell": "/bin/sh",
                    "arguments": ["-i"],
                    "environment": env,
                },
                "cols": 80,
                "rows": 24,
                "initialOutputStreamBytes": 0,
                "maxBacklogBytes": 65536,
            }
            send_frame(self.command.stdin, b"I", json.dumps(request).encode())
            self.command.stdin.close()
            response = self.command.stdout.read(5)
            if len(response) != 5 or response[:1] != b"R":
                raise AssertionError(f"{label} Worker failed to start: {response!r}")
            self.shell_pid = int.from_bytes(response[1:], "little")
            assert self.shell_pid != 0
            self.command.stdout.close()
            self.attach()
        except BaseException:
            self.stop()
            raise

    def record(self):
        file = self.runtime / "pty-workers" / (
            hashlib.sha256(self.session.encode()).hexdigest() + ".json"
        )
        record = json.loads(file.read_text(encoding="utf-8"))
        assert record["workerPid"] == self.command.pid
        assert record["shellPid"] == self.shell_pid
        assert file.stat().st_mode & 0o077 == 0, "Unix Worker token is not private"
        assert file.parent.stat().st_mode & 0o077 == 0
        return record

    def attach(self):
        record = self.record()
        stream = socket.create_connection(("127.0.0.1", record["port"]), timeout=5)
        try:
            stream.settimeout(5)
            request = {
                "sessionId": self.session,
                "token": record["token"],
                "resumeCursor": 0,
            }
            send_frame(stream, b"A", json.dumps(request).encode())
            header = receive_exact(stream, 13)
            assert header[:1] == b"R"
            assert int.from_bytes(header[1:5], "little") == self.shell_pid
        except BaseException:
            stream.close()
            raise
        self.socket = stream
        return stream

    def command_marker(self, name):
        marker = self.runtime / (self.label + "-" + name + ".done")
        token = "ALERA_" + name
        marker.unlink(missing_ok=True)
        line = f"printf '%s' {shlex.quote(token)} > {shlex.quote(str(marker))}\n"
        send_frame(self.socket, b"W", line.encode())
        await_file(marker, token)

    def stop(self):
        if self.socket is not None:
            self.socket.close()
            self.socket = None
        if self.command.poll() is None:
            self.command.kill()
        self.command.wait(timeout=10)
        for pipe in (self.command.stdin, self.command.stdout):
            if pipe is not None and not pipe.closed:
                pipe.close()


def main(exe):
    if os.name != "posix":
        raise SystemExit("Only Linux/macOS support this smoke")
    binary = Path(exe).resolve()
    if not binary.is_file():
        raise FileNotFoundError(binary)

    with tempfile.TemporaryDirectory(prefix="alera-unix-worker-") as directory:
        workers = []
        try:
            first = Worker(binary, directory, "crashing")
            workers.append(first)
            second = Worker(binary, directory, "surviving")
            workers.append(second)
            first.command_marker("FIRST_BEFORE_CRASH")
            second.command_marker("SECOND_BEFORE_CRASH")

            cwd_file = Path(directory) / "actual-shell-cwd.txt"
            send_frame(second.socket, b"W", (
                f"pwd > {shlex.quote(str(cwd_file))}\n"
            ).encode())
            until = time.monotonic() + COMMAND_TIMEOUT
            while not cwd_file.is_file() and time.monotonic() < until:
                time.sleep(0.05)
            assert cwd_file.is_file(), "Unix Worker shell did not report its cwd"
            assert Path(cwd_file.read_text(encoding="utf-8").strip()).resolve() == Path(directory).resolve(), (
                "Unix Worker shell ignored the workspace launch directory"
            )

            # Forcefully terminate only one Worker. The sibling Worker must
            # still own its existing PTY and accept commands.
            first.command.kill()
            first.command.wait(timeout=10)
            second.command_marker("SECOND_AFTER_PEER_CRASH")

            # Simulate Host loss, then authenticate from a replacement Host
            # and prove the original shell survived with the same PID.
            old_pid = second.command.pid
            old_shell = second.shell_pid
            offline_done = second.runtime / "offline.done"
            send_frame(second.socket, b"W", (
                f"sleep 1; printf 'ALERA_OFFLINE_REPLAY' > "
                f"{shlex.quote(str(offline_done))}; "
                f"printf 'ALERA_OFFLINE_REPLAY\\n'\n"
            ).encode())
            second.socket.shutdown(socket.SHUT_RDWR)
            second.socket.close()
            second.socket = None
            await_file(offline_done, "ALERA_OFFLINE_REPLAY")
            assert second.command.poll() is None
            second.attach()
            assert second.command.pid == old_pid
            assert second.shell_pid == old_shell
            replayed = bytearray()
            until = time.monotonic() + 10
            while b"ALERA_OFFLINE_REPLAY" not in replayed and time.monotonic() < until:
                try:
                    replayed.extend(second.socket.recv(4096))
                except socket.timeout:
                    break
            assert b"ALERA_OFFLINE_REPLAY" in replayed, (
                "replacement Host did not receive offline PTY output"
            )
            second.command_marker("SECOND_AFTER_HOST_RESTART")
            print(
                "PASS: Unix PTY Worker crash isolated, original Shell PID "
                "survived Host loss, private token verified, offline replay "
                "and reattached shell input succeeded"
            )
        finally:
            for worker in reversed(workers):
                worker.stop()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: test_terminal_worker_unix.py PATH_TO_ALERA_EXE")
    main(sys.argv[1])
