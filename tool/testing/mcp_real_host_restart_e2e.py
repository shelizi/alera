"""Real Windows Runtime Host process restart and stale MCP capability E2E.

Uses a disposable runtime, project, workspace, and two actual Host-owned PTYs
created before/after restarting the *OS Host process*. The one-time PTY proof
is handed to this test driver's memory over loopback and is NEVER persisted,
printed, or reused in another runtime. Unlike an in-process actor unit test,
this verifies the real Host shutdown/restart boundary.

No production Alera runtime, global CLI configuration, or model account is
accessed. Only the isolated Host this script started may be stopped here.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

from mcp_ui_agent_desktop_e2e import HostRpc

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build" / "windows" / "shared_gui_ninja"
BASE = BUILD / "e2e_host_restart"
RUNTIME = BASE / "runtime"
WORKSPACE = BASE / "workspace"
RESULT = BASE / "mcp_real_host_restart_result.json"
WORKSPACE_ID = "mcp-real-restart-isolated-workspace"
PROJECT_ID = "mcp-real-restart-isolated-project"
HANDLE = "mcp-real-restart-agent-session"


def cli(exe: Path, *args: str) -> object:
    run = subprocess.run(
        [str(exe), *args], capture_output=True, text=True,
        encoding="utf-8", errors="replace", timeout=32,
    )
    if run.returncode:
        # Never forward stdout/stderr from the real Host (may contain tokens).
        raise RuntimeError(f"isolated Alera CLI {args[0]} returned {run.returncode}")
    return json.loads(run.stdout)


def host_status(exe: Path) -> bool:
    return bool(cli(exe, "runtime", "--runtime-dir", str(RUNTIME),
                    "--json", "status").get("running"))


def spawn_host(exe: Path) -> None:
    # The persistent Host is a child of this short-lived starter on Windows.
    # Capturing stdout here holds a pipe open in the Host and deadlocks the
    # parent's subprocess.run(). Use DEVNULL, the same proven strategy as the
    # existing mcp_live_host_smoke.ps1 Start-Process acceptance harness.
    starter = subprocess.Popen(
        [str(exe), "runtime", "--runtime-dir", str(RUNTIME), "--json", "start"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    try:
        starter.wait(timeout=22)
    except subprocess.TimeoutExpired:
        starter.kill()
        raise AssertionError("isolated Host startup CLI did not exit")
    if starter.returncode:
        raise AssertionError(f"isolated Host startup CLI exited {starter.returncode}")
    deadline = time.monotonic() + 18
    while time.monotonic() < deadline:
        if host_status(exe):
            return
        time.sleep(0.25)
    raise AssertionError("the disposable Runtime Host never became ready")


def stop_host(exe: Path) -> None:
    # The Host correctly refuses a normal stop with active Agent PTYs.
    # Force is intentional for this *pinned disposable runtime only*: an OS
    # restart must kill its first PTY before fresh launch rotates the bearer.
    if RUNTIME.resolve() != (BUILD / "e2e_host_restart" / "runtime").resolve():
        raise RuntimeError("refusing to force-stop an unrecognized runtime")
    cli(exe, "runtime", "--runtime-dir", str(RUNTIME), "--json", "stop", "--force")
    deadline = time.monotonic() + 18
    while time.monotonic() < deadline:
        if not host_status(exe):
            return
        time.sleep(0.25)
    raise AssertionError("the disposable Runtime Host did not stop")


def capture_inside_pty(port: int) -> None:
    """Only executed by the fresh Host-managed PTY, never by the driver."""
    token = os.environ.get("ALERA_MCP_UI_TOKEN", "")
    handle = os.environ.get("ALERA_TERMINAL_HANDLE", "")
    workspace = os.environ.get("ALERA_WORKSPACE_ID", "")
    if len(token) != 64 or handle != HANDLE or workspace != WORKSPACE_ID:
        raise RuntimeError("no Host-issued isolated Agent identity in PTY")
    with socket.create_connection(("127.0.0.1", port), timeout=8) as sender:
        sender.sendall(json.dumps({
            "token": token,
            "handle": handle,
            "workspace": workspace,
            "pid": os.getpid(),
        }).encode("utf-8"))
    # Do not print the capability or any secret.
    print("MCP_RESTART_AGENT_CAPTURED", flush=True)


def capture_token(exe: Path) -> dict:
    with socket.create_server(("127.0.0.1", 0), backlog=1) as listener:
        listener.settimeout(15)
        port = listener.getsockname()[1]
        command = f'python -B -u "{Path(__file__).resolve()}" --capture {port}'
        cli(exe, "terminal", "--runtime-dir", str(RUNTIME),
            "--json", "write", "--handle", HANDLE,
            "--text", command, "--enter")
        conn, address = listener.accept()
        if address[0] != "127.0.0.1":
            conn.close()
            raise AssertionError("PTY proof came from non-loopback peer")
        with conn:
            conn.settimeout(6)
            chunks = []
            while True:
                part = conn.recv(512)
                if not part:
                    break
                chunks.append(part)
            result = json.loads(b"".join(chunks).decode("utf-8"))
        if (result.get("handle") != HANDLE or
                result.get("workspace") != WORKSPACE_ID or
                len(result.get("token", "")) != 64):
            raise AssertionError("received unexpected PTY identity")
        return result


def proof(token: str) -> dict:
    return {
        "sessionId": HANDLE,
        "workspaceId": WORKSPACE_ID,
        "capability": token,
    }


def must_deny(host: HostRpc, token: str) -> None:
    try:
        host.request("agentControl.scope", proof(token))
    except RuntimeError:
        return
    raise AssertionError("stale bearer capability was accepted after Host restart")


def run_acceptance(exe: Path) -> None:
    if os.name != "nt":
        raise RuntimeError("the real OS Host restart fixture is Windows-specific")
    if "feat-alera-mcp-control-20261102" not in str(ROOT):
        raise RuntimeError("refusing to run outside the pinned MCP feature worktree")
    if not exe.is_file() or not (BUILD / "runner" / "Debug" / "alera-dev.exe").is_file():
        raise RuntimeError("use the already-built isolated Debug Alera sidecar")
    # Even 'runtime status' initializes SQLite when the profile is missing.
    # Do not call it before checking the directory freshness.
    if RUNTIME.exists():
        # Do not affect even an older test run's private stored data.
        raise RuntimeError("fresh disposable Host directory is required")

    BASE.mkdir(parents=True, exist_ok=True)
    WORKSPACE.mkdir(parents=True, exist_ok=True)
    (WORKSPACE / "sentinel.txt").write_text("MCP isolated restart acceptance\n", encoding="utf-8")
    cases: list[str] = []
    first_started = False
    try:
        spawn_host(exe)
        first_started = True
        cli(exe, "project", "--runtime-dir", str(RUNTIME), "--json", "add",
            "--id", PROJECT_ID, "--name", "MCP disposable restart project",
            "--repo-path", str(WORKSPACE), "--kind", "folder")
        cli(exe, "workspace", "--runtime-dir", str(RUNTIME),
            "--json", "register", "--id", WORKSPACE_ID,
            "--project-id", PROJECT_ID, "--name", "MCP restart workspace",
            "--path", str(WORKSPACE), "--kind", "main")
        now = datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
        host = HostRpc(RUNTIME)
        try:
            host.request("tab.upsert", {
                "id": HANDLE, "workspaceId": WORKSPACE_ID,
                "kind": "terminal", "title": "MCP disposable restart Agent",
                "createdAt": now, "updatedAt": now,
                "payload": {
                    "terminalSessionId": HANDLE,
                    "agentType": "codex", "spawnOnCreate": True,
                },
            })
            old = capture_token(exe)
            before = host.request("agentControl.scope", proof(old["token"]))
            if before.get("authorized") is not True:
                raise AssertionError("first live PTY proof failed scoped auth")
            cases.append("first_host_minted_token_accepted")
            print("MCP_RESTART_PASS first_host_minted_token_accepted", flush=True)
        finally:
            host.close()
        stop_host(exe)
        cases.append("real_host_os_process_stopped")
        print("MCP_RESTART_PASS real_host_os_process_stopped", flush=True)

        spawn_host(exe)
        host = HostRpc(RUNTIME)
        try:
            must_deny(host, old["token"])
            cases.append("stale_token_rejected_after_real_restart")
            print("MCP_RESTART_PASS stale_token_rejected_after_real_restart", flush=True)
            fresh = capture_token(exe)
            if fresh["token"] == old["token"]:
                raise AssertionError("new PTY reused old capability")
            current = host.request("agentControl.scope", proof(fresh["token"]))
            if current.get("authorized") is not True:
                raise AssertionError("Host did not accept fresh PTY's bearer")
            cases.append("same_handle_new_pty_capability_accepted")
            print("MCP_RESTART_PASS same_handle_new_pty_capability_accepted", flush=True)
            must_deny(host, old["token"])
            cases.append("old_token_stays_revoked")
            print("MCP_RESTART_PASS old_token_stays_revoked", flush=True)
        finally:
            host.close()
        RESULT.write_text(json.dumps({
            "passed": len(cases), "cases": cases,
            "hostProcessRestart": True,
            "samePersistentWorkspaceAndHandle": True,
            "privateTestOnly": True,
        }, indent=2), encoding="utf-8")
        print(f"MCP_RESTART_SUMMARY {len(cases)} passed", flush=True)
    finally:
        if first_started and host_status(exe):
            stop_host(exe)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--capture", type=int)
    opts = parser.parse_args()
    if opts.capture is not None:
        capture_inside_pty(opts.capture)
    else:
        if opts.cli is None:
            parser.error("--cli is required for driver")
        try:
            run_acceptance(opts.cli.resolve())
        except Exception as error:
            print(f"MCP_RESTART_FAILURE {type(error).__name__}: {str(error)[:180]}", file=sys.stderr, flush=True)
            raise SystemExit(1)
