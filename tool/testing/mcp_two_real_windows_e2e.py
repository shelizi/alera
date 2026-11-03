"""Two REAL disposable Flutter Shells -> Host-owned PTY -> MCP routing E2E.

The outer process never receives ALERA_MCP_UI_TOKEN; uiControl.windows and
MCP action calls run only inside the Host-owned Agent PTY via the inner test.
No user profile, production single-instance behavior or global MCP config used.
"""
from __future__ import annotations

import ctypes
from ctypes import wintypes
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from mcp_ui_agent_desktop_e2e import HostRpc
from mcp_ui_desktop_fixture import (
    BUILD, RUNTIME, WORKSPACE, WORKSPACE_ID, SESSION_ID, TAB_ID,
)

EXE = BUILD / "runner" / "Debug" / "alera-dev.exe"
CLI = EXE.parent / "resources" / "alera" / "alera.exe"
INNER = Path(__file__).with_name("mcp_two_real_flutter_windows_e2e.py")
INNER_REPORT = BUILD / "mcp_two_real_flutter_windows_result.json"
INNER_FAILURE = BUILD / "mcp_two_real_flutter_failure.json"
REPORT = BUILD / "mcp_two_real_windows_result.json"
PHASE = "module"


def cli(*args: str) -> dict:
    completed = subprocess.run(
        [str(CLI), *args], capture_output=True, encoding="utf-8",
        text=True, errors="replace", timeout=30,
    )
    if completed.returncode:
        raise AssertionError("isolated CLI command failed: " + str(args[:2]))
    data = json.loads(completed.stdout)
    if not isinstance(data, dict):
        raise AssertionError("CLI reply is not an object")
    return data


def host_running() -> bool:
    return cli("runtime", "--runtime-dir", str(RUNTIME),
               "--json", "status").get("running") is True


def visible_window_count(pid: int) -> int:
    callback_type = ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
    user32 = ctypes.windll.user32
    total = 0

    def inspect(hwnd, _):
        nonlocal total
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            total += 1
        return True

    callback = callback_type(inspect)
    user32.EnumWindows(callback, 0)
    return total


def wait_gui(child: subprocess.Popen, label: str) -> None:
    deadline = time.monotonic() + 55
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise AssertionError(label + " exited before HWND ready")
        if visible_window_count(child.pid):
            return
        time.sleep(0.25)
    raise AssertionError(label + " never opened native HWND")


def wait_host() -> None:
    deadline = time.monotonic() + 35
    while time.monotonic() < deadline:
        if host_running():
            return
        time.sleep(0.3)
    raise AssertionError("disposable Runtime Host did not start")


def spawn_agent() -> None:
    now = datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
    tab = {
        "id": TAB_ID, "workspaceId": WORKSPACE_ID, "kind": "terminal",
        "title": "MCP two-window disposable Agent PTY",
        "createdAt": now, "updatedAt": now,
        "payload": {
            "terminalSessionId": SESSION_ID, "agentType": "codex",
            "spawnOnCreate": True,
        },
    }
    rpc = HostRpc(RUNTIME)
    try:
        if rpc.request("tab.upsert", tab).get("id") != TAB_ID:
            raise AssertionError("disposable Agent PTY tab was not registered")
    finally:
        rpc.close()
    deadline = time.monotonic() + 25
    while time.monotonic() < deadline:
        items = cli("terminal", "--runtime-dir", str(RUNTIME),
                    "--json", "list", "--workspace", WORKSPACE_ID).get("items", [])
        if any(item.get("handle") == SESSION_ID and item.get("running") is True
               for item in items):
            return
        time.sleep(0.25)
    raise AssertionError("Host failed to spawn scoped Agent PTY")


def run_in_real_agent() -> dict:
    INNER_REPORT.unlink(missing_ok=True)
    INNER_FAILURE.unlink(missing_ok=True)
    command = subprocess.list2cmdline([sys.executable, str(INNER)])
    # Restrict failure markers to output produced after this test starts.
    # Terminal scrollback can contain tracebacks from older invocations.
    snapshot = cli("terminal", "--runtime-dir", str(RUNTIME), "--json",
                   "read", "--handle", SESSION_ID, "--max-bytes", "1024")
    cursor = snapshot.get("nextCursor")
    if not isinstance(cursor, int):
        raise AssertionError("disposable terminal did not provide a read cursor")
    cli("terminal", "--runtime-dir", str(RUNTIME), "--json",
        "write", "--handle", SESSION_ID, "--text", command, "--enter")
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        if INNER_REPORT.is_file():
            report = json.loads(INNER_REPORT.read_text(encoding="utf-8"))
            if report.get("passed", 0) < 13 or report.get("realFlutterShells") is not True:
                raise AssertionError("real two-Shell MCP acceptance report invalid")
            return report
        output = cli("terminal", "--runtime-dir", str(RUNTIME), "--json",
                     "read", "--handle", SESSION_ID, "--cursor", str(cursor),
                     "--max-bytes", "8192")
        text = str(output.get("text", ""))
        if isinstance(output.get("nextCursor"), int):
            cursor = output["nextCursor"]
        if "TWO_REAL_FLUTTER_FAILURE" in text:
            raise AssertionError("current inner Agent MCP acceptance failed")
        time.sleep(0.3)
    raise AssertionError("real two-Shell MCP result timed out")


def main() -> None:
    global PHASE
    PHASE = "preflight_files"
    if (os.name != "nt" or "feat-alera-mcp-control-20261102" not in str(BUILD)
            or not EXE.is_file() or not CLI.is_file() or not INNER.is_file()
            or not (RUNTIME / "runtime.sqlite").is_file()
            or not (WORKSPACE / "src/deep/needle.dart").is_file()):
        raise RuntimeError("refusing non-isolated or incomplete Windows fixture")
    # Reusing an earlier prebuilt CLI can silently put a non-MCP sidecar in
    # the new Flutter bundle. Reject it before starting disposable windows.
    PHASE = "preflight_cli"
    check = subprocess.run([str(CLI), "mcp", "--help"], capture_output=True,
                           timeout=12, check=False)
    if check.returncode != 0:
        raise RuntimeError("isolated GUI contains a stale alera.exe without MCP; rebuild Rust sidecar")
    PHASE = "preflight_host"
    if host_running():
        raise RuntimeError("disposable Host was already running; refusing to touch it")
    children: list[subprocess.Popen] = []
    checks: list[str] = []
    result = {"passed": False, "checks": checks, "disposableProfileOnly": True}
    host_started = False
    phase = "start_primary"
    try:
        PHASE = phase
        env = dict(os.environ, ALERA_MCP_PROFILE_INJECTION="0")
        primary = subprocess.Popen(
            [str(EXE)], cwd=str(EXE.parent), env=env,
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        children.append(primary)
        wait_gui(primary, "primary")
        wait_host()
        host_started = True
        checks.append("primary_native_shell_and_test_host")

        phase = "start_secondary"
        PHASE = phase
        second = subprocess.Popen(
            [str(EXE), "--alera-mcp-e2e-second-shell"],
            cwd=str(EXE.parent), env=env, stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        children.append(second)
        wait_gui(second, "second")
        if primary.poll() is not None or primary.pid == second.pid:
            raise AssertionError("two independent native Flutter processes required")
        checks.append("second_native_shell")

        phase = "spawn_agent"
        PHASE = phase
        spawn_agent()
        checks.append("host_minted_agent_pty")
        phase = "run_mcp_inside_pty"
        PHASE = phase
        inner = run_in_real_agent()
        checks.extend(inner["checks"])
        result["passed"] = True
    except Exception as error:
        # Persist only fixed stage/class for diagnostics, never PTY contents.
        result["failurePhase"] = phase
        result["failureType"] = type(error).__name__
        raise
    finally:
        PHASE = "cleanup_children"
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
        # Closing both real Flutter GUIs can also stop their isolated Host.
        # A concurrent CLI stop may fail because the Host *already* exited.
        # Do not lose the original E2E result to that harmless shutdown race.
        PHASE = "cleanup_host"
        try:
            if host_started and host_running():
                try:
                    cli("runtime", "--runtime-dir", str(RUNTIME),
                        "--json", "stop", "--force")
                except AssertionError:
                    # Recheck actual status rather than trusting CLI stop exit.
                    if host_running():
                        result["hostStopFailed"] = True
        except Exception:
            result["hostStopFailed"] = True
        PHASE = "cleanup_status"
        # CLI stop can return before the isolated Windows Host process exits.
        # Poll for actual process exit before declaring a cleanup failure.
        stopped = False
        until = time.monotonic() + 20
        while time.monotonic() < until:
            try:
                if not host_running():
                    stopped = True
                    break
            except Exception:
                pass
            time.sleep(0.3)
        result["hostShutdownConfirmed"] = stopped
        if not stopped:
            result["functionalChecksPassed"] = bool(result["passed"])
            result["passed"] = False
        PHASE = "cleanup_report"
        REPORT.write_text(json.dumps(result, indent=2), encoding="utf-8")
        if not stopped:
            raise AssertionError("isolated Host did not shut down")
    print("MCP_TWO_REAL_WINDOWS " + ("PASS" if result["passed"] else "FAIL")
          + " checks=" + str(len(checks)), flush=True)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print("MCP_TWO_REAL_WINDOWS_ERROR " + type(error).__name__ +
              " stage=" + PHASE,
              file=sys.stderr, flush=True)
        sys.exit(1)
