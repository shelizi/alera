"""Windows Host-process crash/restart integration test for PTY recovery.

Usage:
  python tool/debug/test_runtime_host_worker_recovery.py D:/c/t/debug/alera.exe

Uses a fresh runtime in the current worktree, never an installed Alera profile.
Keeps the PTY Worker feature explicitly opted in for the test only.
"""
import base64
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import uuid

START_TIMEOUT = 45


def now_iso():
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


class Host:
    def __init__(self, exe, runtime):
        self.runtime = Path(runtime)
        self.control = self.runtime / "host-control.json"
        self.token = str(uuid.uuid4())
        env = dict(os.environ)
        env["ALERA_TERMINAL_WORKER_PROCESS"] = "1"
        self.child = subprocess.Popen(
            [str(exe), "runtime-host",
             "--runtime-dir", str(runtime),
             "--control-file", str(self.control),
             "--token", self.token,
             "--persistent", "--log-level", "error"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=env,
        )
        deadline = time.monotonic() + START_TIMEOUT
        while time.monotonic() < deadline:
            if self.child.poll() is not None:
                raise AssertionError(f"Host exited during startup: {self.child.returncode}")
            try:
                record = json.loads(self.control.read_text(encoding="utf-8"))
                if record["pid"] == self.child.pid and record["token"] == self.token:
                    break
            except (OSError, KeyError, ValueError):
                pass
            time.sleep(0.1)
        else:
            raise TimeoutError("Host did not publish its control file")

        self.socket = socket.create_connection(("127.0.0.1", record["port"]), timeout=10)
        self.socket.settimeout(55)
        self.reader = self.socket.makefile("rb")
        self.writer = self.socket.makefile("wb")
        self.next_id = 1
        self.request("hello", {
            "protocolVersion": record["protocolVersion"],
            "token": self.token,
            "clientKind": "app",
        })

    def request(self, method, payload):
        req_id = self.next_id
        self.next_id += 1
        wire = json.dumps({"id": req_id, "type": method, "payload": payload}).encode() + b"\n"
        self.writer.write(wire)
        self.writer.flush()
        deadline = time.monotonic() + 55
        while time.monotonic() < deadline:
            line = self.reader.readline()
            if not line:
                raise EOFError(f"Host connection closed during {method}")
            value = json.loads(line)
            if value.get("id") != req_id:
                continue
            if not value.get("ok"):
                raise AssertionError(f"{method} failed: {value.get('error')!r}")
            return value.get("payload")
        raise TimeoutError(f"{method}: no response")

    def kill(self):
        if self.child.poll() is None:
            self.child.kill()
        self.child.wait(timeout=10)
        for obj in (getattr(self, "reader", None), getattr(self, "writer", None)):
            if obj is not None:
                try:
                    obj.close()
                except OSError:
                    pass
        if hasattr(self, "socket"):
            self.socket.close()


def worker_record(runtime, session_id):
    path = Path(runtime) / "pty-workers" / (
        hashlib.sha256(session_id.encode()).hexdigest() + ".json"
    )
    return json.loads(path.read_text(encoding="utf-8"))


def main():
    if sys.platform != "win32" or len(sys.argv) != 2:
        raise SystemExit("Windows only. Usage: test_runtime_host_worker_recovery.py PATH_TO_ALERA_EXE")
    exe = Path(sys.argv[1]).resolve()
    if not exe.is_file():
        raise FileNotFoundError(exe)

    with tempfile.TemporaryDirectory(prefix="alera-host-recovery-") as temporary:
        runtime = Path(temporary)
        hosts = []
        session_id = "host-crash-" + uuid.uuid4().hex[:16]
        workspace_id = "workspace-" + uuid.uuid4().hex[:16]
        tab_id = "tab-" + uuid.uuid4().hex[:16]
        project_id = "project-" + uuid.uuid4().hex[:16]
        timestamp = now_iso()
        cwd = str(Path.cwd())
        record = None
        try:
            host = Host(exe, runtime)
            hosts.append(host)

            host.request("project.upsert", {
                "id": project_id, "name": "Host recovery test",
                "repoPath": cwd, "kind": "folder",
                "createdAt": timestamp, "updatedAt": timestamp,
            })
            host.request("workspace.upsert", {
                "id": workspace_id, "instanceId": workspace_id, "hostId": "local",
                "projectId": project_id, "name": "Host recovery test",
                "branch": None, "path": cwd,
                "createdAt": timestamp, "updatedAt": timestamp,
                "kind": "main", "status": "active", "sourceBranch": None,
                "reusesExistingBranch": False, "parentWorkspaceId": None,
            })
            host.request("tab.upsert", {
                "id": tab_id, "workspaceId": workspace_id, "kind": "terminal",
                "title": "Test terminal", "createdAt": timestamp,
                "updatedAt": timestamp, "payload": {"spawnOnCreate": False},
            })
            terminal = {
                "sessionId": session_id, "workspaceId": workspace_id, "tabId": tab_id,
                "workingDirectory": cwd,
                "launch": {
                    "label": "cmd",
                    "shell": os.environ.get("ComSpec", "cmd.exe"),
                    "arguments": ["/Q", "/K"],
                    "environment": dict(os.environ),
                },
                "cols": 80, "rows": 24,
            }
            created = host.request("createOrAttach", terminal)
            assert created["running"] is True, "initial terminal not running"
            record = worker_record(runtime, session_id)
            # Token registry and directory must not inherit access for Users,
            # Everyone or Authenticated Users from the wider runtime profile.
            registry_file = runtime / "pty-workers" / (
                hashlib.sha256(session_id.encode()).hexdigest() + ".json"
            )
            for secured in (registry_file.parent, registry_file):
                proc = subprocess.run(
                    ["icacls", str(secured)], capture_output=True,
                    text=True, encoding="utf-8", errors="replace", timeout=10,
                )
                assert proc.returncode == 0, f"cannot inspect registry ACL: {proc.stderr}"
                acl = proc.stdout.upper()
                assert "(I)" not in acl, f"registry ACL unexpectedly inherited: {proc.stdout}"
                assert "EVERYONE:" not in acl and "BUILTIN\\USERS:" not in acl, (
                    f"registry ACL permits generic users: {proc.stdout}"
                )
            assert record["schemaVersion"] == 2, "old unprotected token schema"
            original_worker_pid = record["workerPid"]
            original_shell_pid = record["shellPid"]
            assert original_worker_pid > 0 and original_shell_pid > 0

            # Crash only this isolated test Host; do not send graceful stop.
            host.kill()
            time.sleep(1)
            host2 = Host(exe, runtime)
            hosts.append(host2)
            attached = host2.request("createOrAttach", terminal)
            record2 = worker_record(runtime, session_id)
            assert attached["running"] is True
            assert record2["workerPid"] == original_worker_pid, (
                "Host crash caused a second PTY worker to spawn"
            )
            assert record2["shellPid"] == original_shell_pid, (
                "Host crash replaced the original Shell"
            )

            # Verify the recovered Terminal still accepts input.
            host2.request("write", {
                "sessionId": session_id,
                "dataBase64": base64.b64encode(b"echo ALERA_HOST_RECONNECTED\r").decode(),
            })
            time.sleep(0.5)
            read = host2.request("terminal.read", {"sessionId": session_id, "maxBytes": 8192})
            assert "ALERA_HOST_RECONNECTED" in read.get("text", ""), (
                f"Recovered Terminal did not echo through Host: {read!r}"
            )
            print(
                "PASS: actual runtime-host killed and restarted; "
                f"Worker {original_worker_pid} and Shell {original_shell_pid} preserved; terminal input works"
            )
        finally:
            for host in reversed(hosts):
                host.kill()
            # Windows keeps localhost sockets / SQLite handles briefly after
            # a forced Host exit. Allow handles to settle before temp cleanup.
            # Tear down only the recorded Worker created under this test runtime.
            if record is not None:
                try:
                    pid = int(record["workerPid"])
                    safe_exe = str(exe).replace("'", "''")
                    command = (
                        f"$p = Get-CimInstance Win32_Process -Filter 'ProcessId={pid}'; "
                        f"if ($null -ne $p -and $p.ExecutablePath -eq '{safe_exe}' "
                        f"-and $p.CommandLine -like '*__alera-terminal-pty-worker*') "
                        f"{{ Stop-Process -Id {pid} -Force }}"
                    )
                    subprocess.run(
                        ["pwsh", "-NoProfile", "-Command", command],
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10,
                    )
                except (OSError, subprocess.TimeoutExpired):
                    pass
            # Detect lingering test-only processes before TemporaryDirectory
            # removes its runtime; never kill any unrelated application.
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                if record is None:
                    break
                try:
                    import ctypes
                    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
                    kernel32.OpenProcess.argtypes = [
                        ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32
                    ]
                    kernel32.OpenProcess.restype = ctypes.c_void_p
                    kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
                    kernel32.CloseHandle.restype = ctypes.c_int
                    handle = kernel32.OpenProcess(0x00100000, False, int(record["workerPid"]))
                    if not handle:
                        break
                    kernel32.CloseHandle(handle)
                except (OSError, ValueError):
                    break
                time.sleep(0.1)
            time.sleep(0.3)


if __name__ == "__main__":
    main()
