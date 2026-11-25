"""Isolated Windows native ResourceSampler/Worker restart conformance.

Run only an alera.exe built with --features isolated-host-conformance.
The test never uses the installed runtime or changes its sampling default.
"""
import argparse
import base64
import ctypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import statistics
import subprocess
import sys
import tempfile
import time
import uuid

from test_runtime_host_worker_recovery import Host, now_iso, worker_record


def own_creation():
    from ctypes import wintypes

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    kernel.GetProcessTimes.argtypes = [
        wintypes.HANDLE, *([ctypes.POINTER(wintypes.FILETIME)] * 4)
    ]
    kernel.GetProcessTimes.restype = wintypes.BOOL
    values = [wintypes.FILETIME() for _ in range(4)]
    assert kernel.GetProcessTimes(
        kernel.GetCurrentProcess(), *(ctypes.byref(v) for v in values)
    )
    return (values[0].dwHighDateTime << 32) | values[0].dwLowDateTime


def records(runtime):
    path = runtime / "logs" / "runtime.log"
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except FileNotFoundError:
        return []
    result = []
    for line in lines:
        try:
            result.append(json.loads(line))
        except ValueError:
            pass
    return result


class IsolatedHost(Host):
    def __init__(self, exe, runtime):
        self.runtime = runtime
        self.control = runtime / "host-control.json"
        self.token = uuid.uuid4().hex
        env = dict(os.environ)
        for name in (
            "ALERA_TERMINAL_WORKER_PROCESS", "CLAUDE_CONFIG_DIR", "CODEX_HOME",
            "COPILOT_HOME", "GROK_HOME", "OPENCODE_CONFIG_DIR",
            "PI_CODING_AGENT_DIR", "AMP_CONFIG_DIR",
        ):
            env.pop(name, None)
        home = runtime / "isolated-home"
        roaming = home / "AppData" / "Roaming"
        local = home / "AppData" / "Local"
        for path in (roaming, local):
            path.mkdir(parents=True, exist_ok=True)
        env.update({
            "HOME": str(home), "USERPROFILE": str(home),
            "APPDATA": str(roaming), "LOCALAPPDATA": str(local),
            "XDG_CONFIG_HOME": str(roaming),
            "ALERA_TEST_ISOLATED_HOST": "1",
            "ALERA_RESOURCE_NATIVE_WINDOWS": "1",
            "ALERA_CLOUD_URL": "http://127.0.0.1:9",
            "ALERA_HOST_LOG": (
                "warn,resource_conformance=debug,"
                "alera::terminal_host::resources::windows_native=debug"
            ),
        })
        self.child = subprocess.Popen(
            [str(exe), "runtime-host", "--runtime-dir", str(runtime),
             "--control-file", str(self.control), "--token", self.token,
             "--persistent"],
            env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                assert self.child.poll() is None, "private Host exited"
                try:
                    record = json.loads(self.control.read_text(encoding="utf-8"))
                    if record["pid"] == self.child.pid and record["token"] == self.token:
                        break
                except (OSError, KeyError, ValueError):
                    pass
                time.sleep(0.1)
            else:
                raise TimeoutError("private Host control file")
            self.socket = socket.create_connection(("127.0.0.1", record["port"]), timeout=10)
            self.socket.settimeout(15)
            self.reader = self.socket.makefile("rb")
            self.writer = self.socket.makefile("wb")
            self.next_id = 1
            self.request("hello", {
                "protocolVersion": record["protocolVersion"],
                "token": self.token, "clientKind": "app",
            })
            assert any(
                r.get("msg") == "isolated host skipped global integration writes"
                for r in records(runtime)
            ), "missing isolation acknowledgement; refusing to create PTYs"
        except BaseException:
            self.kill()
            raise


def create_sessions(host, runtime, count=2):
    timestamp = now_iso()
    host.request("project.upsert", {
        "id": "private-project", "name": "Private resource test",
        "repoPath": str(runtime), "kind": "folder",
        "createdAt": timestamp, "updatedAt": timestamp,
    })
    host.request("workspace.upsert", {
        "id": "private-workspace", "instanceId": "private-workspace", "hostId": "local",
        "projectId": "private-project", "name": "Private resource test",
        "branch": None, "path": str(runtime), "createdAt": timestamp,
        "updatedAt": timestamp, "kind": "main", "status": "active",
        "sourceBranch": None, "reusesExistingBranch": False, "parentWorkspaceId": None,
    })
    requests = []
    for i in range(count):
        tab = f"private-tab-{i}"
        host.request("tab.upsert", {
            "id": tab, "workspaceId": "private-workspace", "kind": "terminal",
            "title": "Private resource test", "createdAt": timestamp,
            "updatedAt": timestamp, "payload": {"spawnOnCreate": False},
        })
        request = {
            "sessionId": f"private-session-{i}", "workspaceId": "private-workspace",
            "tabId": tab, "workingDirectory": str(runtime),
            "launch": {"label": "cmd", "shell": os.environ.get("ComSpec", "cmd.exe"),
                       "arguments": ["/Q", "/K"], "environment": {}},
            "cols": 80, "rows": 24,
        }
        assert host.request("createOrAttach", request)["running"]
        requests.append(request)
    return requests


def identity(runtime, request):
    rec = worker_record(runtime, request["sessionId"])
    assert rec["schemaVersion"] == 2
    return rec["workerPid"], rec["shellPid"], rec.get("shellCreationFiletime")


def exercise_session(host, request, marker):
    path = host.runtime / "command-complete.txt"
    path.unlink(missing_ok=True)
    command = f'cmd /d /c echo {marker} > "{path}"\r'
    host.request("write", {
        "sessionId": request["sessionId"],
        "dataBase64": base64.b64encode(command.encode()).decode(),
    })
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        try:
            if path.read_text(encoding="utf-8").strip() == marker:
                return
        except (FileNotFoundError, PermissionError):
            pass
        time.sleep(0.05)
    raise AssertionError("recovered shell did not execute marker command")


def sample(host, requests, phase, expected, target=6, soak_seconds=0):
    request = {"appPid": os.getpid(), "appCreationFiletime": str(own_creation()),
               "intervalMs": 2000}
    last = 0
    frames = []
    timings = []
    started = time.monotonic()
    deadline = started + 75 + soak_seconds
    next_exercise = started + 5
    exercise_count = 0
    expected_streak = 0
    while time.monotonic() < deadline:
        snap = host.request("resources.snapshot", request)
        if not snap or snap.get("collectedAt", 0) <= last:
            time.sleep(0.25)
            continue
        rows = snap.get("sessions", [])
        if len(rows) != len(requests):
            time.sleep(0.25)
            continue
        last = snap["collectedAt"]
        assert all(row["running"] and row["measured"] and row["memoryBytes"] > 0
                   and row["processCount"] >= 1 for row in rows), phase
        assert sorted(row["shellPid"] for row in rows) == sorted(
            identity(host.runtime, r)[1] for r in requests)
        assert snap["processes"]["app"]["memoryBytes"] > 0
        total = sum(row["memoryBytes"] for row in rows)
        total += sum(snap["processes"][k]["memoryBytes"] for k in ("host", "app"))
        assert snap["totals"]["memoryBytes"] == total
        record = next(
            (r for r in reversed(records(host.runtime))
             if r.get("msg") == "resource snapshot completed"
             and str(r.get("collected_at")) == str(last)), None)
        assert record is not None, "missing aggregate sample timing"
        backend = record["backend"]
        frames.append(backend)
        if backend == expected and not snap.get("warming"):
            expected_streak += 1
            timings.append(int(record["elapsed_us"]) / 1000)
        else:
            expected_streak = 0
        now = time.monotonic()
        if soak_seconds and now >= next_exercise and now < started + soak_seconds:
            exercise_count += 1
            exercise_session(host, requests[exercise_count % len(requests)],
                             f"ALERA_SOAK_{exercise_count}")
            next_exercise = now + 5
        if expected_streak >= target and now >= started + soak_seconds:
            print(f"PASS {phase}: frames={len(frames)} native={frames.count('native')} "
                  f"sysinfo={frames.count('sysinfo')} settled_median_ms="
                  f"{statistics.median(timings):.3f} commands={exercise_count} "
                  f"elapsed_s={now - started:.1f}", flush=True)
            return
        time.sleep(0.25)
    raise AssertionError(f"{phase}: missing {target} consecutive settled {expected} frames; "
                         f"native={frames.count('native')} sysinfo={frames.count('sysinfo')}")


def stop_private_workers(exe, runtime):
    # The executable is a unique private copy. Match both its path and the
    # Worker invocation, including records from partially completed setup.
    pids = set()
    for path in (runtime / "pty-workers").glob("*.json"):
        try:
            pids.add(int(json.loads(path.read_text(encoding="utf-8"))["workerPid"]))
        except (OSError, ValueError, KeyError):
            pass
    if not pids:
        return
    script = (
        "$exe = '" + str(exe).replace("'", "''") + "'; "
        "foreach ($id in @(" + ",".join(str(p) for p in pids) + ")) { "
        "$p = Get-CimInstance Win32_Process -Filter ('ProcessId=' + $id); "
        "if ($null -ne $p -and $p.ExecutablePath -eq $exe -and "
        "$p.CommandLine -like '*__alera-terminal-pty-worker*') "
        "{ Stop-Process -Id $id -Force -ErrorAction SilentlyContinue } }"
    )
    subprocess.run(["pwsh", "-NoProfile", "-Command", script],
                   capture_output=True, timeout=15, check=False)


def main(exe, soak_seconds):
    assert sys.platform == "win32"
    exe = Path(exe).resolve(strict=True)
    # Never launch a normal/Release binary: its integration dispatcher does
    # not honor the conformance-only isolation environment flag.
    image = exe.read_bytes()
    assert b"ALERA_TEST_ISOLATED_HOST" in image
    assert b"isolated host skipped global integration writes" in image
    del image
    with tempfile.TemporaryDirectory(prefix="alera-resource-worker-") as temp:
        # A private immutable copy prevents other builds replacing the tested
        # executable across restarts and scopes emergency cleanup to this run.
        private_exe = Path(temp) / "alera.exe"
        shutil.copyfile(exe, private_exe)
        exe = private_exe
        runtime = Path(temp) / "runtime"
        runtime.mkdir()
        host = None
        requests = []
        try:
            host = IsolatedHost(exe, runtime)
            requests = create_sessions(host, runtime)
            original = [identity(runtime, r) for r in requests]
            assert all(i[0] > 0 and i[1] > 0 and i[2] for i in original)
            print("PASS default Worker mode: two schema-2 records with precise markers",
                  flush=True)
            sample(host, requests, "before restart", "native")
            host.kill()
            time.sleep(1)
            host = IsolatedHost(exe, runtime)
            for r in requests:
                assert host.request("createOrAttach", r)["running"]
            assert [identity(runtime, r) for r in requests] == original
            exercise_session(host, requests[0], "ALERA_RESTART_RECOVERED")
            print("PASS Host restart: original Worker/Shell/FILETIME preserved", flush=True)
            sample(host, requests, "after restart", "native",
                   soak_seconds=soak_seconds)

            host.kill()
            time.sleep(1)
            path = runtime / "pty-workers" / (
                hashlib.sha256(requests[0]["sessionId"].encode()).hexdigest() + ".json")
            record = json.loads(path.read_text(encoding="utf-8"))
            assert record.pop("shellCreationFiletime") == original[0][2]
            # In-place only on the private test manifest; preserve its ACL.
            path.write_text(json.dumps(record), encoding="utf-8")
            host = IsolatedHost(exe, runtime)
            for r in requests:
                assert host.request("createOrAttach", r)["running"]
            mixed = [identity(runtime, r) for r in requests]
            assert mixed[0][:2] == original[0][:2] and mixed[0][2] is None
            assert mixed[1] == original[1]
            exercise_session(host, requests[0], "ALERA_LEGACY_RECOVERED")
            sample(host, requests, "mixed legacy and precise records", "sysinfo", target=4)
            print("PASS old record preserved and measured through safe fallback", flush=True)
        finally:
            if host is not None:
                if host.child.poll() is None:
                    for r in requests:
                        try:
                            host.request("terminate", {"sessionId": r["sessionId"]})
                        except Exception:
                            pass
                host.kill()
            stop_private_workers(exe, runtime)
            time.sleep(0.5)


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("conformance requires Python assertions; do not use -O")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable")
    parser.add_argument("--soak-seconds", type=int, default=0)
    args = parser.parse_args()
    if not 0 <= args.soak_seconds <= 300:
        parser.error("--soak-seconds must be between 0 and 300")
    main(args.executable, args.soak_seconds)
