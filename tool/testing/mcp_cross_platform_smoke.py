"""Native macOS/Linux/Windows Alera MCP stdio + isolated Runtime Host smoke.

Run with the *native* Rust CLI for the current operating system:
  python3 tool/testing/mcp_cross_platform_smoke.py --cli rust/target/debug/alera

Never invokes a model, opens the real app, starts the normal user Runtime
Host, edits an existing workspace, or prints/persists the disposable Host
credential. Confirms OS-native PTY data and fail-closed MCP authorization.
Every runtime/project/workspace/terminal belongs to a unique temp directory.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
import uuid

PROTOCOL = "2025-06-18"
REQUEST_TIMEOUT = 38


def cli(exe: Path, *arguments: str) -> dict:
    result = subprocess.run(
        [str(exe), *arguments], capture_output=True, text=True,
        encoding="utf-8", errors="replace", timeout=REQUEST_TIMEOUT,
    )
    if result.returncode:
        # Do not leak Host credentials, PTY data, or CLI arguments into CI logs.
        raise AssertionError(f"Alera {arguments[0]} CLI exited {result.returncode}")
    value = json.loads(result.stdout)
    if not isinstance(value, dict):
        raise AssertionError("Alera CLI reply is not a JSON object")
    return value


def mcp(exe: Path, runtime: Path, requests: list[dict], env: dict[str, str]) -> dict[int, dict]:
    frames = "\n".join(json.dumps(request, separators=(",", ":"))
                       for request in requests) + "\n"
    completed = subprocess.run(
        [str(exe), "mcp", "--runtime-dir", str(runtime), "serve"],
        input=frames, capture_output=True, text=True, encoding="utf-8",
        errors="replace", timeout=REQUEST_TIMEOUT, env=env,
    )
    if completed.returncode:
        raise AssertionError(f"Alera MCP stdio exited {completed.returncode}")
    responses: dict[int, dict] = {}
    for line in completed.stdout.splitlines():
        if not line.strip():
            continue
        value = json.loads(line)
        frame_id = value.get("id")
        if not isinstance(frame_id, int) or frame_id in responses:
            raise AssertionError("Invalid or duplicate MCP JSON-RPC response ID")
        responses[frame_id] = value
    expected = {request["id"] for request in requests if "id" in request}
    if set(responses) != expected:
        raise AssertionError("MCP responded to notifications or skipped a request")
    return responses


def request(number: int, name: str, args: dict | None = None) -> dict:
    return {"jsonrpc": "2.0", "id": number, "method": "tools/call",
            "params": {"name": name, "arguments": args or {}}}


def result(responses: dict[int, dict], number: int, *, error: bool = False) -> dict:
    payload = responses[number].get("result")
    if not isinstance(payload, dict) or payload.get("isError") is not error:
        raise AssertionError(f"MCP tool response {number} has wrong error state")
    return payload.get("structuredContent", {}) if not error else payload


def host_status(exe: Path, runtime: Path) -> bool:
    return bool(cli(exe, "runtime", "--runtime-dir", str(runtime),
                    "--json", "status").get("running"))


def start_host(exe: Path, runtime: Path) -> None:
    # The detached Windows Host inherits the starter's output handles.
    # Never capture them: the parent would wait for the daemon's pipe forever.
    starter = subprocess.Popen(
        [str(exe), "runtime", "--runtime-dir", str(runtime), "--json", "start"],
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        starter.wait(timeout=24)
    except subprocess.TimeoutExpired:
        starter.kill()
        starter.wait(timeout=5)
        raise AssertionError("isolated Host starter did not return")
    if starter.returncode:
        raise AssertionError(f"isolated Host starter exited {starter.returncode}")
    deadline = time.monotonic() + 16
    while time.monotonic() < deadline:
        if host_status(exe, runtime):
            return
        time.sleep(0.2)
    raise AssertionError("isolated Runtime Host never became ready")


def stop_host(exe: Path, runtime: Path) -> None:
    # Only the Host spawned under the caller's fresh tempfile is stopped.
    cli(exe, "runtime", "--runtime-dir", str(runtime), "--json", "stop", "--force")
    deadline = time.monotonic() + 18
    while time.monotonic() < deadline:
        # A still-exiting Windows Host may temporarily lock its SQLite
        # control database. 'runtime status' can exit nonzero during that
        # short shutdown interval; never misclassify it as stopped.
        try:
            if not host_status(exe, runtime):
                return
        except (AssertionError, subprocess.TimeoutExpired, json.JSONDecodeError):
            pass
        time.sleep(0.2)
    raise AssertionError("disposable Runtime Host remained running")


def agent_presence(runtime: Path, handle: str) -> None:
    """Inject a synthetic Agent hook ONLY through the temporary Host socket."""
    control = json.loads((runtime / "runtime-host.json").read_text(encoding="utf-8"))
    with socket.create_connection(("127.0.0.1", int(control["port"])), timeout=8) as sock:
        sock.settimeout(8)
        reader = sock.makefile("r", encoding="utf-8", newline="\n")
        writer = sock.makefile("w", encoding="utf-8", newline="\n")
        try:
            def rpc(request_id: int, kind: str, body: dict) -> dict:
                writer.write(json.dumps(
                    {"id": request_id, "type": kind, "payload": body},
                    separators=(",", ":")) + "\n")
                writer.flush()
                for _ in range(30):
                    line = reader.readline()
                    if not line:
                        break
                    reply = json.loads(line)
                    if reply.get("id") == request_id:
                        if reply.get("ok") is not True:
                            raise AssertionError(f"private Host refused test {kind}")
                        return reply.get("payload") or {}
                raise AssertionError(f"no private Host reply for {kind}")

            rpc(1, "hello", {
                "protocolVersion": control["protocolVersion"],
                "token": control["token"], "clientKind": "cli",
            })
            rpc(2, "orchestration.agentStatus", {"entries": [{
                "terminalSessionId": handle, "state": "working",
                "agentType": "codex", "toolName": "ReadFile",
                "toolInput": "PRIVATE_MCP_AGENT_INPUT_NEVER_EXPOSE",
                "lastAssistantMessage": "Non-sensitive test activity",
            }]})
        finally:
            writer.close()
            reader.close()


def check(value: bool, message: str) -> None:
    if not value:
        raise AssertionError(message)
    print("PASS " + message, flush=True)


def run(exe: Path) -> None:
    exe = exe.resolve(strict=True)
    if not exe.is_file():
        raise AssertionError("native Alera executable is missing")
    safe_env = os.environ.copy()
    for name in (
        "ALERA_TERMINAL_HANDLE", "ALERA_WORKSPACE_ID", "ALERA_MCP_UI_TOKEN",
        "ALERA_MCP_PROFILE_INJECTION", "ALERA_CLI_PATH",
    ):
        safe_env.pop(name, None)

    # An MCP client must not bootstrap a Host simply by listing tools.
    with tempfile.TemporaryDirectory(prefix="alera-mcp-no-host-") as sandbox:
        missing = Path(sandbox) / "runtime-not-started"
        frames = mcp(exe, missing, [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize",
             "params": {"protocolVersion": PROTOCOL, "capabilities": {},
                        "clientInfo": {"name": "alera-native-os", "version": "1"}}},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        ], safe_env)
        check(frames[1].get("result", {}).get("protocolVersion") == PROTOCOL,
              "native MCP stdio initializes without starting Host")
        tools = frames[2].get("result", {}).get("tools", [])
        check(len(tools) == 18 and
              sum(item.get("annotations", {}).get("readOnlyHint") is True
                  for item in tools) == 10 and not missing.exists(),
              "18 MCP tool schemas and 10 read-only; no Host side effects")

    root = Path(tempfile.mkdtemp(prefix="alera-mcp-native-os-")).resolve()
    runtime = root / "private runtime"
    workspace = root / "fixture workspace"  # spaces exercise real OS paths
    project_id = "native-mcp-project-" + uuid.uuid4().hex
    workspace_id = "native-mcp-workspace-" + uuid.uuid4().hex
    host_launch_attempted = False
    safe_to_delete = False
    try:
        (workspace / "src").mkdir(parents=True)
        (workspace / "src" / "mcp_fixture.rs").write_text(
            "// Alera cross-platform MCP read-only test\n", encoding="utf-8")
        host_launch_attempted = True
        start_host(exe, runtime)
        check(host_status(exe, runtime), "native Runtime Host started in private temp directory")

        project = cli(exe, "project", "--runtime-dir", str(runtime), "--json",
                      "add", "--id", project_id, "--name", "Alera MCP native fixture",
                      "--repo-path", str(workspace), "--kind", "folder")
        check(project.get("id") == project_id, "isolated project registration")
        entry = cli(exe, "workspace", "--runtime-dir", str(runtime), "--json",
                    "register", "--id", workspace_id, "--project-id", project_id,
                    "--name", "Native MCP workspace", "--path", str(workspace),
                    "--kind", "main")
        check(entry.get("id") == workspace_id, "isolated workspace registration")
        tab = cli(exe, "tab", "--runtime-dir", str(runtime), "--json", "create",
                  "--workspace-id", workspace_id, "--title", "Native MCP PTY",
                  "--spawn")
        check(tab.get("kind") == "terminal", "native Host-owned terminal tab created")
        handle = None
        deadline = time.monotonic() + 14
        while time.monotonic() < deadline:
            items = cli(exe, "terminal", "--runtime-dir", str(runtime), "--json",
                        "list", "--workspace", workspace_id).get("items", [])
            live = [item for item in items if item.get("running") is True]
            if live:
                handle = live[0]["handle"]
                break
            time.sleep(0.15)
        check(bool(handle), "native OS PTY worker becomes live")
        agent_presence(runtime, handle)
        check(True, "private Host accepted synthetic Agent hook")

        caller_env = dict(safe_env, ALERA_TERMINAL_HANDLE=handle,
                          ALERA_WORKSPACE_ID=workspace_id)
        commands = [
            request(10, "alera.list_projects", {"query": "native fixture", "limit": 10}),
            request(11, "alera.list_workspaces", {"projectId": project_id}),
            request(12, "alera.list_tabs", {"workspaceId": workspace_id}),
            request(13, "alera.search_files", {"query": "mcp_fixture", "limit": 10}),
            request(14, "alera.search_agents", {"query": "codex"}),
            request(15, "alera.get_agent_activity", {"handle": handle}),
            request(16, "alera.read_agent_output", {"handle": handle, "maxBytes": 4096}),
            request(20, "alera.list_tabs", {"workspaceId": "foreign-workspace"}),
            request(21, "alera.read_agent_output", {"handle": "unknown-agent"}),
            request(22, "alera.open_file", {"relativePath": "src/mcp_fixture.rs"}),
            request(23, "alera.focus_agent", {"handle": handle}),
            request(24, "alera.send_agent_message", {
                "toHandle": handle, "subject": "denied", "body": "must deny",
                "clientMutationId": "native-smoke-unbound",
            }),
            request(25, "alera.update_setting", {
                "path": "terminal.fontSize", "expectedValue": 16, "value": 15,
            }),
            request(26, "alera.list_projects", {"limit": 999}),
            request(27, "alera.read_agent_output", {"handle": handle, "maxBytes": 8193}),
        ]
        outputs = mcp(exe, runtime, commands, caller_env)
        check(any(item.get("id") == project_id
                  for item in result(outputs, 10).get("items", [])),
              "MCP reads registered project")
        check(any(item.get("id") == workspace_id
                  for item in result(outputs, 11).get("items", [])),
              "MCP reads registered workspace")
        check(any(item.get("id") == tab.get("id")
                  for item in result(outputs, 12).get("items", [])),
              "MCP reads registered terminal tab")
        check(any(item.get("relativePath") == "src/mcp_fixture.rs"
                  for item in result(outputs, 13).get("items", [])),
              "MCP searches files inside private workspace")
        agents = result(outputs, 14)
        check(any(item.get("handle") == handle
                  for item in agents.get("items", []))
              and "PRIVATE_MCP_AGENT_INPUT_NEVER_EXPOSE" not in json.dumps(agents),
              "MCP agent search is scoped and hides raw tool input")
        activity = result(outputs, 15)
        check(activity.get("agent", {}).get("agentState") == "working"
              and "PRIVATE_MCP_AGENT_INPUT_NEVER_EXPOSE" not in json.dumps(activity),
              "MCP Agent hook is visible but sensitive input is redacted")
        check(result(outputs, 16).get("kind") == "agentOutput",
              "MCP reads bounded native PTY output")
        for number in (20, 21, 22, 23, 24, 25, 26, 27):
            result(outputs, number, error=True)
        check(True, "foreign access, unauthorized mutations and limits denied")

        marker = "MCP_NATIVE_CURSOR_TEST"
        echo = "echo " + marker  # works in POSIX shells and cmd.exe
        cli(exe, "terminal", "--runtime-dir", str(runtime), "--json", "write",
            "--handle", handle, "--text", echo, "--enter")
        deadline = time.monotonic() + 14
        next_cursor = None
        while time.monotonic() < deadline:
            output = result(mcp(exe, runtime, [
                request(30, "alera.read_agent_output", {
                    "handle": handle, "cursor": 0, "maxBytes": 8192,
                }),
            ], caller_env), 30)
            if marker in output.get("text", ""):
                next_cursor = output.get("nextCursor")
                break
            time.sleep(0.15)
        check(isinstance(next_cursor, int), "real native PTY text arrives via MCP")
        incremental = result(mcp(exe, runtime, [
            request(31, "alera.read_agent_output", {
                "handle": handle, "cursor": next_cursor, "maxBytes": 8192,
            }),
        ], caller_env), 31)
        check(incremental.get("cursor", -1) >= next_cursor
              and incremental.get("truncated") is False,
              "native PTY output cursor supports incremental MCP reads")

        forged_env = dict(caller_env, ALERA_TERMINAL_HANDLE="not-an-existing-pty")
        forbidden = mcp(exe, runtime, [request(40, "alera.list_tabs")], forged_env)
        result(forbidden, 40, error=True)
        check(True, "unknown caller PTY identity cannot query live tabs")
        check((workspace / "src" / "mcp_fixture.rs").read_text(encoding="utf-8")
              == "// Alera cross-platform MCP read-only test\n",
              "read-only MCP smoke never changes workspace file")
    finally:
        # A failed starter may still have created a detached Host. Check it
        # even when start_host raised, and never remove a live Host's state.
        try:
            if host_launch_attempted:
                # Even a successful forced stop has a brief SQLite close
                # race. Confirm actual stopped status before cleanup, with a
                # fixed timeout; retain the private fixture if uncertain.
                deadline = time.monotonic() + 18
                while time.monotonic() < deadline:
                    try:
                        running = host_status(exe, runtime)
                    except (AssertionError, subprocess.TimeoutExpired,
                            json.JSONDecodeError):
                        time.sleep(0.25)
                        continue
                    if running:
                        stop_host(exe, runtime)
                    else:
                        safe_to_delete = True
                        break
            else:
                safe_to_delete = True
        finally:
            if safe_to_delete:
                for attempt in range(25):
                    try:
                        shutil.rmtree(root)
                        break
                    except (OSError, PermissionError):
                        if attempt == 24:
                            raise
                        time.sleep(0.2)
    print("MCP_NATIVE_PLATFORM_SMOKE_PASS", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", type=Path, required=True)
    opts = parser.parse_args()
    run(opts.cli)
