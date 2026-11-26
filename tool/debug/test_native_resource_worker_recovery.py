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
    def __init__(self, exe, runtime, legacy_only=False):
        self.runtime = runtime
        self.control = runtime / "host-control.json"
        self.token = uuid.uuid4().hex
        isolation_before = sum(
            r.get("msg") == "isolated host skipped global integration writes"
            for r in records(runtime)
        )
        env = dict(os.environ)
        for name in (
            "ALERA_TERMINAL_WORKER_PROCESS", "ALERA_RESOURCE_NATIVE_WINDOWS",
            "CLAUDE_CONFIG_DIR", "CODEX_HOME",
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
            "ALERA_CLOUD_URL": "http://127.0.0.1:9",
            "ALERA_HOST_LOG": (
                "warn,resource_conformance=debug,"
                "alera::terminal_host::resources::windows_native=debug"
            ),
        })
        # Exercise the actual production default with no override. The
        # legacy-only smoke proves that the explicit escape hatch still works.
        if legacy_only:
            env["ALERA_RESOURCE_NATIVE_WINDOWS"] = "0"
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
            assert sum(
                r.get("msg") == "isolated host skipped global integration writes"
                for r in records(runtime)
            ) > isolation_before, "missing new isolation acknowledgement; refusing to create PTYs"
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


def app_request(creation=None):
    return {"appPid": os.getpid(),
            "appCreationFiletime": str(own_creation() if creation is None else creation),
            "intervalMs": 2000}


def snapshot_if_new(host, requests, payload, last, phase, app_measured=True):
    snap = host.request("resources.snapshot", payload)
    if not snap or snap.get("collectedAt", 0) <= last:
        return None
    rows = snap.get("sessions", [])
    if len(rows) != len(requests):
        return None
    assert {row["sessionId"] for row in rows} == {r["sessionId"] for r in requests}, phase
    assert all(row["running"] and row["measured"] and row["memoryBytes"] > 0
               and row["processCount"] >= 1 for row in rows), phase
    assert {row["sessionId"]: row["shellPid"] for row in rows} == {
        r["sessionId"]: identity(host.runtime, r)[1] for r in requests
    }
    app = snap["processes"]["app"]
    if app_measured:
        assert app["memoryBytes"] > 0 and app["processCount"] >= 1, phase
    else:
        assert app["memoryBytes"] == 0 and app["processCount"] == 0, phase
        assert app["cpuPercent"] == 0, phase
    total = sum(row["memoryBytes"] for row in rows)
    total += sum(snap["processes"][k]["memoryBytes"] for k in ("host", "app"))
    assert snap["totals"]["memoryBytes"] == total
    for row in rows + [snap["processes"]["host"], app]:
        history = row["history"]
        assert 1 <= len(history) <= 60 and history[-1] == row["memoryBytes"], phase
    record = next(
        (r for r in reversed(records(host.runtime))
         if r.get("msg") == "resource snapshot completed"
         and str(r.get("collected_at")) == str(snap["collectedAt"])), None)
    assert record is not None, "missing aggregate sample timing"
    assert record["backend"] in ("native", "sysinfo")
    return snap, record


def events(host, message):
    return [r for r in records(host.runtime) if r.get("msg") == message]


def fallback_events(host):
    return events(host, "resource sampler falling back to sysinfo")


def verify_cooldown(host, requests, last):
    """Force one real rejection, then keep the original ticker alive."""
    creation = own_creation()
    wrong = creation - 1 if creation % 10_000_000 == 9_999_999 else creation + 1
    assert wrong // 10_000_000 == creation // 10_000_000
    correct = app_request(creation)
    invalid = app_request(wrong)
    previous_fallbacks = len(fallback_events(host))
    original = [identity(host.runtime, r) for r in requests]
    original_host = host.child.pid
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        result = snapshot_if_new(host, requests, invalid, last,
                                 "wrong app creation marker", app_measured=False)
        if result:
            rejected, record = result
            assert record["backend"] == "sysinfo"
            last = rejected["collectedAt"]
            break
        time.sleep(0.25)
    else:
        raise AssertionError("wrong marker did not produce a complete fallback frame")
    new_fallbacks = fallback_events(host)[previous_fallbacks:]
    assert len(new_fallbacks) == 1, new_fallbacks
    assert "ShellGeneration" in str(new_fallbacks[0]), new_fallbacks
    rejected_at = last
    # The six settled frames before this forced fallback must not disappear.
    history_lengths = {
        row["sessionId"]: len(row["history"]) for row in rejected["sessions"]
    }
    assert min(history_lengths.values()) >= 6, "fallback discarded session history"
    print("PASS same-second wrong App marker: actual ShellGeneration rejection, "
          "zero App attribution; correct identity restored without restarting ticker",
          flush=True)

    frames = []
    native_warming = 0
    native_streak = 0
    first_native_at = None
    started = time.monotonic()
    deadline = started + 75
    while time.monotonic() < deadline:
        result = snapshot_if_new(host, requests, correct, last, "cooldown recovery")
        if not result:
            time.sleep(0.25)
            continue
        snap, record = result
        last = snap["collectedAt"]
        for row in snap["sessions"]:
            size = len(row["history"])
            assert size >= history_lengths[row["sessionId"]], "recovery reset history"
            history_lengths[row["sessionId"]] = size
        backend = record["backend"]
        frames.append(backend)
        if backend == "native":
            if first_native_at is None:
                first_native_at = last
                # The rejected frame is timestamped after its legacy sweep.
                # Allow two seconds of cadence/sweep tolerance; detect an
                # accidental baseline reset that would resume immediately.
                assert first_native_at - rejected_at >= 28_000, (
                    "native resumed before cooldown", first_native_at - rejected_at
                )
            if snap.get("warming"):
                native_warming += 1
                native_streak = 0
            else:
                native_streak += 1
        else:
            native_streak = 0
        if native_streak >= 4:
            assert frames.count("sysinfo") >= 2, frames
            assert native_warming >= 1, "native warmup not observed after fallback"
            assert host.child.pid == original_host and host.child.poll() is None
            assert [identity(host.runtime, r) for r in requests] == original
            print(f"PASS cooldown recovery: same Host/Workers, retained history, "
                  f"sysinfo={frames.count('sysinfo')} native={frames.count('native')} "
                  f"native_warming={native_warming} "
                  f"first_native_after_ms={first_native_at - rejected_at} "
                  f"elapsed_s={time.monotonic() - started:.1f}", flush=True)
            return last
        time.sleep(0.25)
    raise AssertionError(f"native did not recover after cooldown: {frames}")


def launch_churn_wave(host, requests, wave, children):
    """Queue a bounded command chain in each private PTY, not the real app."""
    pending = []
    for index, request in enumerate(requests):
        marker = f"ALERA_CHURN_{wave}_{index}"
        path = host.runtime / f"{marker}.txt"
        path.unlink(missing_ok=True)
        # One bounded holder overlaps the next sample more reliably than
        # very short cmd processes alone; the remaining children churn.
        commands = [f'"{sys.executable}" -B -c "import time; time.sleep(0.75)"']
        commands.extend(["cmd /d /c exit 0"] * (children - 1))
        commands.append(f'echo {marker} > "{path}"')
        # A marker proves the entire chain succeeded, not just its last cmd.
        command = " && ".join(commands) + "\r"
        host.request("write", {
            "sessionId": request["sessionId"],
            "dataBase64": base64.b64encode(command.encode()).decode(),
        })
        pending.append((path, marker))
    return pending


def completed_wave(pending):
    for path, marker in pending:
        try:
            if path.read_text(encoding="utf-8").strip() != marker:
                return False
        except (FileNotFoundError, PermissionError):
            return False
    return True


def sample(host, requests, phase, expected, target=6, soak_seconds=0,
           churn_children=0, last=0, require_legacy=False):
    request = app_request()
    frames = []
    timings = []
    started = time.monotonic()
    soak_until = started + soak_seconds
    deadline = soak_until + 75
    next_exercise = started + 2
    exercise_count = 0
    waves = 0
    pending = []
    wave_deadline = 0
    expected_streak = 0
    max_post_streak = 0
    post_frames = []
    next_report = started + 30
    peak_session_processes = 0
    fallback_before = len(fallback_events(host))
    retries_before = len(events(host, "native topology retry"))
    schedules_before = len(events(host, "native resource retry scheduled"))
    while time.monotonic() < deadline:
        now = time.monotonic()
        if pending:
            if completed_wave(pending):
                exercise_count += len(pending)
                pending = []
            else:
                assert now < wave_deadline, "private PTY command wave timed out"
        if soak_seconds and now >= next_exercise and now < soak_until and not pending:
            if churn_children:
                waves += 1
                pending = launch_churn_wave(host, requests, waves, churn_children)
                wave_deadline = time.monotonic() + 20
                next_exercise = now + 2
            else:
                exercise_count += 1
                exercise_session(host, requests[exercise_count % len(requests)],
                                 f"ALERA_SOAK_{exercise_count}")
                next_exercise = now + 5
        result = snapshot_if_new(host, requests, request, last, phase)
        if not result:
            time.sleep(0.25)
            continue
        snap, record = result
        last = snap["collectedAt"]
        backend = record["backend"]
        if require_legacy:
            assert backend == "sysinfo", "explicit opt-out used native"
        frames.append(backend)
        peak_session_processes = max(
            peak_session_processes, sum(row["processCount"] for row in snap["sessions"])
        )
        now = time.monotonic()
        post_soak = now >= soak_until and not pending
        if post_soak:
            post_frames.append((backend, bool(snap.get("warming")),
                                int(record["elapsed_us"])))
        if backend == expected and not snap.get("warming"):
            timings.append(int(record["elapsed_us"]) / 1000)
            if post_soak:
                expected_streak += 1
                max_post_streak = max(max_post_streak, expected_streak)
            else:
                expected_streak = 0
        else:
            expected_streak = 0
        if soak_seconds and now >= next_report:
            reasons = [r.get("reason", "?") for r in fallback_events(host)[fallback_before:]]
            print(f"PROGRESS {phase}: elapsed_s={now - started:.1f} "
                  f"commands={exercise_count} waves={waves} pending={len(pending)} "
                  f"native={frames.count('native')} sysinfo={frames.count('sysinfo')} "
                  f"post_streak={expected_streak} reasons={reasons} "
                  f"extra_attempts={len(events(host, 'native topology retry')) - retries_before} "
                  f"scheduled_reasons={[r.get('reason', '?') for r in events(host, 'native resource retry scheduled')[schedules_before:]]}",
                  flush=True)
            next_report = now + 30
        if expected_streak >= target:
            if churn_children:
                assert waves >= 1 and exercise_count == waves * len(requests)
            fallback_count = len(fallback_events(host)) - fallback_before
            print(f"PASS {phase}: frames={len(frames)} native={frames.count('native')} "
                  f"sysinfo={frames.count('sysinfo')} settled_median_ms="
                  f"{statistics.median(timings):.3f} commands={exercise_count} "
                  f"waves={waves} completed_child_invocations={exercise_count * churn_children} "
                  f"peak_session_processes={peak_session_processes} "
                  f"fallback_transitions={fallback_count} "
                  f"reasons={[r.get('reason', '?') for r in fallback_events(host)[fallback_before:]]} "
                  f"extra_attempts={len(events(host, 'native topology retry')) - retries_before} "
                  f"scheduled_reasons={[r.get('reason', '?') for r in events(host, 'native resource retry scheduled')[schedules_before:]]} "
                  f"elapsed_s={now - started:.1f}", flush=True)
            return last
        time.sleep(0.25)
    reasons = [r.get("reason", "?") for r in fallback_events(host)[fallback_before:]]
    raise AssertionError(
        f"{phase}: missing {target} consecutive settled {expected} frames; "
        f"native={frames.count('native')} sysinfo={frames.count('sysinfo')} "
        f"commands={exercise_count} waves={waves} pending={len(pending)} "
        f"max_post_streak={max_post_streak} post_frames={post_frames} reasons={reasons} "
        f"extra_attempts={len(events(host, 'native topology retry')) - retries_before} "
        f"scheduled_reasons={[r.get('reason', '?') for r in events(host, 'native resource retry scheduled')[schedules_before:]]}"
    )


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


def main(exe, soak_seconds, session_count, churn_children, check_cooldown,
         legacy_only=False):
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
            host = IsolatedHost(exe, runtime, legacy_only=legacy_only)
            requests = create_sessions(host, runtime, session_count)
            original = [identity(runtime, r) for r in requests]
            assert all(i[0] > 0 and i[1] > 0 and i[2] for i in original)
            assert len({i[0] for i in original}) == session_count
            assert len({i[1] for i in original}) == session_count
            print(f"PASS default Worker mode: {session_count} schema-2 records with precise markers",
                  flush=True)
            if legacy_only:
                sample(host, requests, "explicit legacy override", "sysinfo",
                       target=4, require_legacy=True)
                return
            last = sample(host, requests, "before restart", "native")
            if check_cooldown:
                verify_cooldown(host, requests, last)
            host.kill()
            time.sleep(1)
            host = IsolatedHost(exe, runtime)
            for r in requests:
                assert host.request("createOrAttach", r)["running"]
            assert [identity(runtime, r) for r in requests] == original
            exercise_session(host, requests[0], "ALERA_RESTART_RECOVERED")
            print("PASS Host restart: original Worker/Shell/FILETIME preserved", flush=True)
            sample(host, requests, "after restart", "native",
                   soak_seconds=soak_seconds, churn_children=churn_children)
            assert [identity(runtime, r) for r in requests] == original

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
            assert mixed[1:] == original[1:]
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
    parser.add_argument("--session-count", type=int, default=2)
    parser.add_argument("--churn-children", type=int, default=0)
    parser.add_argument("--verify-cooldown", action="store_true")
    parser.add_argument("--legacy-only", action="store_true",
                        help="smoke test the explicit sysinfo override")
    args = parser.parse_args()
    if not 0 <= args.soak_seconds <= 300:
        parser.error("--soak-seconds must be between 0 and 300")
    if not 2 <= args.session_count <= 8:
        parser.error("--session-count must be between 2 and 8")
    if not 0 <= args.churn_children <= 12:
        parser.error("--churn-children must be between 0 and 12")
    if args.churn_children and args.soak_seconds < 10:
        parser.error("churn requires at least 10 soak seconds")
    if args.legacy_only and (args.verify_cooldown or args.soak_seconds or args.churn_children):
        parser.error("--legacy-only is a short smoke test without cooldown or churn")
    main(args.executable, args.soak_seconds, args.session_count,
         args.churn_children, args.verify_cooldown, args.legacy_only)
