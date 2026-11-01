"""Validate owner fencing when replacement Hosts attach to one live PTY Worker.

Exercise the real Windows sidecar IPC listener directly under a disposable
runtime. After each authenticated takeover, the old socket must not be able
to execute a shell command. A unique file marker proves command execution,
not ConPTY's input echo.

Usage: python tool/debug/test_terminal_worker_competing_hosts.py D:/c/t/debug/alera.exe
"""
import json
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

from test_terminal_worker_isolation import Worker, receive_exact, write_frame

HANDOVERS = 12
MARKER_WAIT_SECONDS = 8


def await_marker(path, expected):
    deadline = time.monotonic() + MARKER_WAIT_SECONDS
    while time.monotonic() < deadline:
        try:
            if path.read_text(encoding="utf-8").strip() == expected:
                return
        except (FileNotFoundError, PermissionError):
            pass
        time.sleep(0.04)
    raise AssertionError(f"command never executed: {path} (expected={expected})")


def send_marker(sock, marker_file, marker):
    command = f'echo {marker} > "{marker_file}"\r'.encode("utf-8")
    write_frame(sock, b"W", command)


def connect_new_owner(worker):
    record = worker.registry()
    sock = socket.create_connection(("127.0.0.1", record["port"]), timeout=5)
    try:
        sock.settimeout(5)
        write_frame(sock, b"A", json.dumps({
            "sessionId": worker.session_id,
            "token": record["token"],
            "resumeCursor": 0,
        }).encode("utf-8"))
        ack = receive_exact(sock, 13)
        assert ack[:1] == b"R", f"invalid authenticated attach: {ack!r}"
        assert int.from_bytes(ack[1:5], "little") == worker.shell_pid, (
            "takeover reminted the Shell instead of preserving it"
        )
        return sock
    except BaseException:
        sock.close()
        raise


def main(executable):
    if sys.platform != "win32":
        raise SystemExit("Windows only")
    executable = Path(executable).resolve()
    if not executable.is_file():
        raise FileNotFoundError(executable)

    with tempfile.TemporaryDirectory(prefix="alera-worker-owner-fence-", dir=Path.cwd()) as runtime:
        worker = None
        current = None
        try:
            worker = Worker(executable, runtime, "competing-hosts")
            original_worker_pid = worker.process.pid
            original_shell_pid = worker.shell_pid
            current = worker.socket
            marker_root = Path(runtime)

            # Respond to ConPTY startup cursor query before issuing file commands.
            worker.assert_echo("ALERA_OWNER_READY", "ALERA_OWNER_READY")
            initial = marker_root / "initial.txt"
            send_marker(current, initial, "CURRENT")
            await_marker(initial, "CURRENT")

            for i in range(HANDOVERS):
                successor = connect_new_owner(worker)
                expected_file = marker_root / f"accepted-{i}.txt"
                denied_file = marker_root / f"stale-{i}.txt"

                # Old socket is not trustworthy after successor authenticated,
                # regardless of whether write_frame itself returns success.
                try:
                    send_marker(current, denied_file, f"STALE_{i}")
                except (OSError, ConnectionError):
                    pass
                send_marker(successor, expected_file, f"ACTIVE_{i}")
                await_marker(expected_file, f"ACTIVE_{i}")
                time.sleep(0.12)
                assert not denied_file.exists(), (
                    f"stale Host {i} executed input after takeover"
                )
                current.close()
                current = successor
                assert worker.process.poll() is None
                assert worker.process.pid == original_worker_pid
                assert worker.shell_pid == original_shell_pid

            print(
                f"PASS: {HANDOVERS} authenticated Host takeovers preserve "
                "Worker/Shell PID; every obsolete Host rejected from PTY input"
            )
        finally:
            if current is not None:
                current.close()
            if worker is not None:
                worker.close()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: test_terminal_worker_competing_hosts.py PATH_TO_ALERA_EXE")
    main(sys.argv[1])
