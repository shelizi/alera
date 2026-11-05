"""Real Claude model -> MCP send -> Host durable inbox -> recipient Claude response.

Windows acceptance only. Both model turns execute in separate Host-owned
disposable Agent PTYs. A bounded recipient *adapter* reports idle only after
its real first model turn exits; this does not replace native Claude status
hook / Managed Profile acceptance and MUST NOT be described as such.

No global Agent MCP config, user workspace file, or production runtime writes.
All provider transcripts and Host-minted bearer credentials stay out of reports.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import uuid

from mcp_ui_agent_desktop_e2e import HostRpc, call_mcp

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build" / "windows" / "shared_gui_ninja"
CLI = BUILD / "runner" / "Debug" / "resources" / "alera" / "alera.exe"
RESULT = BUILD / "mcp_real_model_collaboration_result.json"
WORKSPACE_ID = "mcp-real-model-collaboration-workspace"
PROJECT_ID = "mcp-real-model-collaboration-project"
SENDER = "mcp-real-model-sender"
RECEIVER = "mcp-real-model-recipient"
SERVER = "alera-workbench"
SEND_TOOL = "mcp__alera-workbench__alera_send_agent_message"


def cli(*args: str) -> dict:
    proc = subprocess.run([str(CLI), *args], capture_output=True, text=True,
                          encoding="utf-8", errors="replace", timeout=30)
    if proc.returncode:
        raise AssertionError(f"disposable Alera CLI failed ({args[0]})")
    body = json.loads(proc.stdout)
    if not isinstance(body, dict):
        raise AssertionError("CLI returned non-object JSON")
    return body


def status(runtime: Path) -> bool:
    return cli("runtime", "--runtime-dir", str(runtime),
               "--json", "status").get("running") is True


def start_host(runtime: Path) -> None:
    starter = subprocess.Popen(
        [str(CLI), "runtime", "--runtime-dir", str(runtime), "--json", "start"],
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    try:
        starter.wait(timeout=24)
    except subprocess.TimeoutExpired:
        starter.kill()
        starter.wait(timeout=5)
        raise AssertionError("disposable Host start timed out")
    if starter.returncode:
        raise AssertionError("disposable Host start failed")
    for _ in range(70):
        if status(runtime):
            return
        time.sleep(0.2)
    raise AssertionError("disposable Host did not become ready")


def stop_host(runtime: Path) -> None:
    cli("runtime", "--runtime-dir", str(runtime), "--json", "stop", "--force")
    for _ in range(70):
        if not status(runtime):
            return
        time.sleep(0.2)
    raise AssertionError("disposable Host did not stop")


def process_model(workspace: Path, prompt: str, *, tool_config: str | None = None) -> dict:
    claude = shutil.which("claude.exe")
    if not claude:
        raise RuntimeError("Claude Code executable not installed")
    args = [
        claude, "--no-session-persistence", "--setting-sources", "",
        "--tools", "", "--max-turns", "3", "--max-budget-usd", "0.10",
        "--output-format", "stream-json", "--verbose", "--print",
    ]
    if tool_config:
        args[2:2] = [
            "--strict-mcp-config", "--mcp-config", tool_config,
            "--allowedTools", SEND_TOOL,
        ]
    args.append(prompt)
    try:
        proc = subprocess.run(
            args, cwd=str(workspace), stdin=subprocess.DEVNULL,
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=90,
        )
    except subprocess.TimeoutExpired as error:
        raise AssertionError("real Claude model turn timed out") from error
    events = []
    for line in proc.stdout.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(event, dict):
            events.append(event)
    return {"exit": proc.returncode, "events": events}


def marker_from_result(events: list[dict], marker: str) -> bool:
    return any(
        event.get("type") == "result"
        and event.get("is_error") is not True
        and marker in str(event.get("result", ""))
        for event in events
    )


def inside_pty(expected: str, runtime: Path, workspace: Path) -> None:
    if (os.name != "nt" or "feat-alera-mcp-control-20261102" not in str(ROOT)
            or os.environ.get("ALERA_TERMINAL_HANDLE") != expected
            or os.environ.get("ALERA_WORKSPACE_ID") != WORKSPACE_ID
            or len(os.environ.get("ALERA_MCP_UI_TOKEN", "")) != 64
            or not workspace.is_dir() or not (runtime / "runtime.sqlite").is_file()):
        raise RuntimeError("real model test must run in the disposable Host-owned Agent PTY")


def receiver(runtime: Path, workspace: Path, state: Path, nonce: str) -> None:
    inside_pty(RECEIVER, runtime, workspace)
    # Mark busy just before an ACTUAL provider model turn. No fake quick
    # working->done update is permitted before the model's result exists.
    host = HostRpc(runtime)
    try:
        host.request("orchestration.agentStatus", {"entries": [
            {"terminalSessionId": RECEIVER, "agentType": "claude", "state": "working"},
        ]})
        preflight = process_model(
            workspace, "Safe disposable Alera model integration smoke. "
                       "Reply exactly RECEIVER_MODEL_READY. No tools or edits.",
        )
        if preflight["exit"] != 0 or not marker_from_result(
                preflight["events"], "RECEIVER_MODEL_READY"):
            raise AssertionError("recipient's first real model turn did not finish")
        host.request("orchestration.agentStatus", {"entries": [
            {"terminalSessionId": RECEIVER, "agentType": "claude", "state": "done"},
        ]})
        (state / "receiver_idle.json").write_text(
            json.dumps({"realModelIdleTurn": True}), encoding="utf-8",
        )
        print("MCP_REAL_RECIPIENT_IDLE_AFTER_MODEL", flush=True)
    finally:
        host.close()

    # The real Host will paste its formatted Orchestration message into
    # this PTY and submit Enter. This adapter only consumes that PTY input
    # and hands the actual prompt to a NEW real Claude model turn.
    deadline = time.monotonic() + 95
    observed = False
    while time.monotonic() < deadline:
        raw = sys.stdin.readline()
        if not raw:
            raise AssertionError("recipient Agent PTY stdin closed")
        if nonce in raw:
            observed = True
            break
    if not observed:
        raise AssertionError("recipient PTY never received matching MCP message")
    reply_marker = "RECEIVER_MODEL_ACK_" + nonce
    response = process_model(
        workspace,
        "Alera disposable Agent-to-Agent message was delivered via your Host "
        "Terminal PTY. The message was marked with nonce " + nonce + ". "
        "Respond exactly " + reply_marker + ". Do not use tools, edit files, "
        "or assume that delivery alone means completion.",
    )
    passed = response["exit"] == 0 and marker_from_result(
        response["events"], reply_marker)
    (state / "recipient_reply.json").write_text(json.dumps({
        "passed": passed, "provider": "claude",
        "actualProviderModelReply": passed, "receivedFromActualHostPty": observed,
        "nativeProviderHooksVerified": False,
    }, indent=2), encoding="utf-8")
    if not passed:
        raise AssertionError("real recipient model did not return the exact reply marker")
    print("MCP_REAL_RECIPIENT_MODEL_REPLIED", flush=True)


def sender(runtime: Path, workspace: Path, state: Path, nonce: str) -> None:
    inside_pty(SENDER, runtime, workspace)
    mutation = "mcp-model-collab-" + nonce
    body = "Please acknowledge this isolated test message nonce " + nonce
    expected = {
        "toHandle": RECEIVER, "subject": "Disposable collaboration acceptance",
        "body": body, "clientMutationId": mutation,
    }
    config = json.dumps({"mcpServers": {SERVER: {
        "command": str(CLI.resolve()),
        "args": ["mcp", "--runtime-dir", str(runtime.resolve()), "serve"],
    }}}, separators=(",", ":"))
    prompt = (
        "Use exactly ONE MCP tool: " + SEND_TOOL + " with arguments "
        + json.dumps(expected, separators=(",", ":")) + ". This is an "
        "isolated real two-Agent integration test. Do not use another tool, "
        "shell, or file edits. Only report what the actual tool returns."
    )
    run = process_model(workspace, prompt, tool_config=config)
    calls: set[str] = set()
    correlated = 0
    for event in run["events"]:
        message = event.get("message")
        if not isinstance(message, dict):
            continue
        blocks = message.get("content")
        if not isinstance(blocks, list):
            continue
        if event.get("type") == "assistant":
            for item in blocks:
                if (isinstance(item, dict) and item.get("type") == "tool_use"
                        and item.get("name") == SEND_TOOL
                        and item.get("input") == expected
                        and isinstance(item.get("id"), str)):
                    calls.add(item["id"])
        elif event.get("type") == "user":
            for item in blocks:
                if (isinstance(item, dict) and item.get("type") == "tool_result"
                        and item.get("tool_use_id") in calls
                        and item.get("is_error") is not True
                        and "messageId" in json.dumps(item.get("content", ""))):
                    correlated += 1
    # Independently inspect durable status through the exact sender Agent's
    # scoped MCP tool. Model prose or "delivered" alone never proves a reply.
    sent = call_mcp(CLI, runtime, "alera.send_agent_message", expected)
    receipt = sent.get("structuredContent") or {}
    msg_id = receipt.get("messageId")
    if not isinstance(msg_id, str) or not msg_id:
        raise AssertionError("no durable sender-owned message receipt")
    deadline = time.monotonic() + 30
    final_status = ""
    while time.monotonic() < deadline:
        current = call_mcp(CLI, runtime, "alera.get_agent_message_status",
                           {"messageId": msg_id})
        data = current.get("structuredContent") or {}
        final_status = str(data.get("deliveryStatus", ""))
        if final_status == "delivered":
            if data.get("completionConfirmed") is not False:
                raise AssertionError("delivery was incorrectly marked complete")
            break
        time.sleep(0.2)
    passed = (run["exit"] == 0 and len(calls) == 1 and correlated == 1
              and final_status == "delivered")
    (state / "sender_result.json").write_text(json.dumps({
        "passed": passed, "realModelIssuedMcpSendCalls": len(calls),
        "correlatedToolReceipts": correlated, "durableMessageIdPresent": True,
        "deliveryStatus": final_status, "completionConfirmed": False,
    }, indent=2), encoding="utf-8")
    if not passed:
        raise AssertionError("real sender model / MCP tool receipt was not confirmed")
    print("MCP_REAL_SENDER_SENT_AND_DELIVERED", flush=True)


def terminal_write(runtime: Path, command: list[str], handle: str) -> None:
    cli("terminal", "--runtime-dir", str(runtime), "--json", "write",
        "--handle", handle, "--text", subprocess.list2cmdline(command), "--enter")


def wait_file(path: Path, *, timeout: float, label: str) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.is_file():
            return json.loads(path.read_text(encoding="utf-8"))
        time.sleep(0.25)
    raise AssertionError(label + " was never reported from Host Agent PTY")


def main_driver() -> None:
    if (os.name != "nt" or "feat-alera-mcp-control-20261102" not in str(ROOT)
            or not CLI.is_file() or not shutil.which("claude.exe")):
        raise RuntimeError("requires isolated Windows Alera MCP and authenticated Claude CLI")
    BUILD.mkdir(parents=True, exist_ok=True)
    outcome = {"passed": False, "checks": [], "nativeProviderHooksVerified": False,
               "crossWorkspaceGrantUsed": False, "scope": "disposable"}
    try:
        with tempfile.TemporaryDirectory(prefix="mcp_real_collab_", dir=BUILD, ignore_cleanup_errors=True) as td:
            base = Path(td)
            runtime, workspace = base / "runtime", base / "workspace"
            workspace.mkdir()
            (workspace / "sentinel.txt").write_text(
                "Real model Agent collaboration test; no user data.\n", encoding="utf-8")
            nonce = uuid.uuid4().hex[:12]
            started = False
            try:
                start_host(runtime)
                started = True
                cli("project", "--runtime-dir", str(runtime), "--json",
                    "add", "--id", PROJECT_ID, "--name", "Disposable model collaboration",
                    "--repo-path", str(workspace), "--kind", "folder")
                cli("workspace", "--runtime-dir", str(runtime), "--json",
                    "register", "--id", WORKSPACE_ID, "--project-id", PROJECT_ID,
                    "--name", "Disposable", "--path", str(workspace), "--kind", "main")
                host = HostRpc(runtime)
                try:
                    now = datetime.now(timezone.utc).isoformat(
                        timespec="milliseconds").replace("+00:00", "Z")
                    for handle in (RECEIVER, SENDER):
                        host.request("tab.upsert", {
                            "id": handle, "workspaceId": WORKSPACE_ID,
                            "kind": "terminal", "title": "Disposable " + handle,
                            "createdAt": now, "updatedAt": now,
                            "payload": {
                                "terminalSessionId": handle,
                                "agentType": "claude", "spawnOnCreate": True,
                            },
                        })
                finally:
                    host.close()
                for _ in range(80):
                    items = cli("terminal", "--runtime-dir", str(runtime), "--json",
                                "list", "--workspace", WORKSPACE_ID).get("items", [])
                    if all(any(x.get("handle") == handle and x.get("running")
                               for x in items) for handle in (RECEIVER, SENDER)):
                        break
                    time.sleep(0.25)
                else:
                    raise AssertionError("two Host-owned Agent PTYs were not ready")
                outcome["checks"].append("two_real_host_agent_ptys")

                command = [sys.executable, "-B", "-u", str(Path(__file__).resolve()),
                           "--mode", "receiver", "--runtime", str(runtime),
                           "--workspace", str(workspace), "--state", str(base),
                           "--nonce", nonce]
                terminal_write(runtime, command, RECEIVER)
                ready = wait_file(base / "receiver_idle.json",
                                  timeout=110, label="real model idle")
                if ready.get("realModelIdleTurn") is not True:
                    raise AssertionError("receiver not model-confirmed idle")
                outcome["checks"].append("receiver_real_model_returned_idle")

                command[command.index("receiver")] = "sender"
                terminal_write(runtime, command, SENDER)
                sent = wait_file(base / "sender_result.json",
                                 timeout=120, label="sender model MCP receipt")
                if not sent.get("passed"):
                    raise AssertionError("sender model did not produce validated MCP send")
                outcome["checks"].append("sender_real_model_mcp_tool_send")
                if sent.get("deliveryStatus") != "delivered":
                    raise AssertionError("Host PTY delivery not confirmed")
                outcome["checks"].append("host_durable_inbox_delivered")
                received = wait_file(base / "recipient_reply.json",
                                     timeout=120, label="receiver model reply")
                if not received.get("passed"):
                    raise AssertionError("recipient model failed to reply")
                outcome["checks"].append("receiver_real_model_replied")
                outcome["passed"] = True
            finally:
                if started and status(runtime):
                    stop_host(runtime)
        # Windows may hold SQLite or ConPTY files briefly after Host shutdown.
        for attempt in range(25):
            if not base.exists():
                break
            try:
                shutil.rmtree(base)
            except (PermissionError, OSError):
                time.sleep(0.4)
        if base.exists():
            raise AssertionError("isolated runtime cleanup incomplete")
        outcome["cleanupConfirmed"] = True
    except Exception as error:
        outcome["passed"] = False
        outcome["failureType"] = type(error).__name__
    RESULT.write_text(json.dumps(outcome, indent=2), encoding="utf-8")
    print("MCP_REAL_MODEL_COLLAB " +
          ("PASS" if outcome["passed"] else "FAIL") +
          " checks=" + str(len(outcome["checks"])), flush=True)
    if not outcome["passed"]:
        raise AssertionError("real Agent model collaboration E2E did not pass")


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--mode", choices=("driver", "receiver", "sender"),
                   default="driver")
    p.add_argument("--runtime", type=Path)
    p.add_argument("--workspace", type=Path)
    p.add_argument("--state", type=Path)
    p.add_argument("--nonce")
    args = p.parse_args()
    if args.mode == "driver":
        main_driver()
        return
    if not all((args.runtime, args.workspace, args.state, args.nonce)):
        raise RuntimeError("Agent modes require isolated runtime/workspace/state/nonce")
    if args.mode == "receiver":
        receiver(args.runtime, args.workspace, args.state, args.nonce)
    else:
        sender(args.runtime, args.workspace, args.state, args.nonce)


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print("MCP_REAL_MODEL_COLLAB_ERROR " + type(exc).__name__,
              file=sys.stderr, flush=True)
        raise SystemExit(1)
