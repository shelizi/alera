"""Real disposable Alera Host Hook HTTP -> queued MCP message -> recipient PTY test.

This exercises the *native Hook endpoint* and actual Host-owned ConPTY
delivery, not a synthetic orchestration.agentStatus request. The recipient is
an inert Python stdin reader, NOT an interactive Claude/Codex model. No global
Agent settings/credentials/workspaces may be changed.
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
from urllib.error import HTTPError
from urllib.request import Request, urlopen
import uuid

from mcp_real_model_collaboration_e2e import (
    BUILD, CLI, PROJECT_ID, ROOT, SENDER, RECEIVER, WORKSPACE_ID,
    cli, status, stop_host, terminal_write, wait_file,
)
from mcp_ui_agent_desktop_e2e import HostRpc, call_mcp

RESULT = BUILD / "mcp_native_hook_host_delivery_result.json"


def private_host_start(runtime: Path, private_home: Path) -> None:
    private_home.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    # Runtime Host integration reconciliation, even when disabled, may
    # reconcile settings under home. Redirect both Windows/POSIX home only
    # for this isolated Host and its child PTYs; never touch user configs.
    env["USERPROFILE"] = str(private_home)
    env["HOME"] = str(private_home)
    p = subprocess.Popen(
        [str(CLI), "runtime", "--runtime-dir", str(runtime), "--json", "start"],
        env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    try:
        p.wait(timeout=30)
    except subprocess.TimeoutExpired:
        p.kill()
        p.wait(timeout=6)
        raise AssertionError("private Host startup timed out")
    if p.returncode:
        raise AssertionError("private Host startup failed")
    for _ in range(90):
        if status(runtime):
            return
        time.sleep(0.2)
    raise AssertionError("private Host not ready")


def assert_private(runtime: Path, workspace: Path, handle: str) -> None:
    if (
        os.name != "nt"
        or "feat-alera-mcp-control-20261102" not in str(ROOT)
        or os.environ.get("ALERA_TERMINAL_HANDLE") != handle
        or os.environ.get("ALERA_WORKSPACE_ID") != WORKSPACE_ID
        or len(os.environ.get("ALERA_MCP_UI_TOKEN", "")) != 64
        or not (workspace / "sentinel.txt").is_file()
        or not (runtime / "runtime.sqlite").is_file()
    ):
        raise RuntimeError("test must run inside intended private Host-owned Agent PTY")


def native_http(runtime: Path, event: str, *, token_override: str | None = None,
                workspace_override: str | None = None, tab_override: str | None = None) -> int:
    endpoint = runtime / "agent-hooks" / "endpoint.cmd"
    if not endpoint.is_file():
        raise AssertionError("private Host native Hook endpoint missing")
    fields: dict[str, str] = {}
    for line in endpoint.read_text(encoding="utf-8").splitlines():
        if line.startswith("set ALERA_AGENT_HOOK_") and "=" in line:
            k, v = line[4:].split("=", 1)
            fields[k] = v.strip()
    port = int(fields["ALERA_AGENT_HOOK_PORT"])
    bearer = token_override if token_override is not None else fields["ALERA_AGENT_HOOK_TOKEN"]
    if not (0 < port < 65536 and bearer):
        raise AssertionError("private native Hook endpoint is invalid")
    body = json.dumps({
        "terminalSessionId": RECEIVER,
        "workspaceId": workspace_override or WORKSPACE_ID,
        "tabId": tab_override or RECEIVER,
        "hookEventName": event,
        "version": "1",
        "payload": {
            "hook_event_name": event,
            "session_id": "native-http-disposable-turn",
            # This suite only tests native Hook state and PTY delivery.
            # Suppress unrelated automatic LLM title jobs, which otherwise
            # keep a temporary Windows cwd alive after Host shutdown.
            "agentTitleIgnore": True,
        },
    }, separators=(",", ":")).encode("utf-8")
    req = Request(
        f"http://127.0.0.1:{port}/hook/claude", body,
        headers={
            "Content-Type": "application/json",
            "X-Alera-Agent-Hook-Token": bearer,
        },
        method="POST",
    )
    try:
        with urlopen(req, timeout=6) as response:
            return response.status
    except HTTPError as error:
        return error.code


def receiver(runtime: Path, workspace: Path, state: Path, nonce: str) -> None:
    assert_private(runtime, workspace, RECEIVER)
    (state / "receiver_ready.json").write_text('{"ready":true}', encoding="utf-8")
    # The Host writes the Orchestration prompt into this exact live ConPTY,
    # while Python is reading it as DATA. Never interpret injected text as a
    # command, unlike a bare shell, and never persist actual PTY input.
    for line in sys.stdin:
        if nonce in line:
            (state / "recipient_received.json").write_text(
                '{"receivedThroughHostPty":true}', encoding="utf-8",
            )
            print("MCP_NATIVE_HOOK_RECIPIENT_RECEIVED", flush=True)
            return
    raise AssertionError("recipient PTY stdin closed without queued message")


def sender(runtime: Path, workspace: Path, state: Path, nonce: str) -> None:
    assert_private(runtime, workspace, SENDER)
    def stage(name: str) -> None:
        (state / "sender_stage.json").write_text(
            json.dumps({"phase": name}), encoding="utf-8",
        )
    stage("native_working")

    def mcp(name: str, args: dict) -> dict:
        response = call_mcp(CLI, runtime, name, args)
        if response.get("isError") is not False:
            raise AssertionError("scoped MCP call failed")
        return response.get("structuredContent") or {}

    if native_http(runtime, "UserPromptSubmit") != 204:
        raise AssertionError("native UserPromptSubmit HTTP was not accepted")
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        activity = mcp("alera.get_agent_activity", {"handle": RECEIVER})
        if (activity.get("agent") or {}).get("agentState") == "working":
            break
        time.sleep(0.2)
    else:
        raise AssertionError("native UserPromptSubmit did not mark Host Agent working")
    stage("queue_mcp")
    receipt = mcp("alera.send_agent_message", {
        "toHandle": RECEIVER,
        "subject": "Private native HTTP Hook acceptance",
        "body": "Read this inert private test message nonce " + nonce,
        "clientMutationId": "native-hook-" + nonce,
    })
    msg_id = receipt.get("messageId")
    if not isinstance(msg_id, str) or not msg_id:
        raise AssertionError("Host durable message receipt missing")
    if receipt.get("deliveryStatus") != "queued":
        raise AssertionError("message to busy peer was not queued")

    def delivery() -> str:
        return str(mcp("alera.get_agent_message_status", {"messageId": msg_id})
                   .get("deliveryStatus", ""))

    stage("verify_queued")
    if delivery() != "queued":
        raise AssertionError("queued message was delivered before native Stop")
    stage("reject_invalid_token")
    if native_http(runtime, "Stop", token_override="invalid-private-token") != 403:
        raise AssertionError("native HTTP receiver accepted an invalid bearer")
    stage("reject_forged_workspace_and_tab")
    if native_http(runtime, "Stop", workspace_override="foreign-workspace") != 204:
        raise AssertionError("native Hook receiver rejected well-formed test payload")
    if native_http(runtime, "Stop", tab_override="foreign-tab") != 204:
        raise AssertionError("native Hook receiver rejected well-formed test payload")
    time.sleep(0.3)
    if delivery() != "queued":
        raise AssertionError("forged native Hook triggered PTY message injection")

    stage("legitimate_native_stop")
    if native_http(runtime, "Stop") != 204:
        raise AssertionError("legitimate native Stop HTTP was not accepted")
    deadline = time.monotonic() + 25
    while time.monotonic() < deadline:
        if delivery() == "delivered":
            break
        time.sleep(0.2)
    else:
        raise AssertionError("legitimate native Hook did not trigger real PTY delivery")
    result = mcp("alera.get_agent_message_status", {"messageId": msg_id})
    if result.get("completionConfirmed") is not False:
        raise AssertionError("delivered message was incorrectly marked complete")
    stage("write_result")
    (state / "sender_result.json").write_text(
        json.dumps({
            "scopedMcpSender": True,
            "unauthorizedBearerRejected": True,
            "forgedWorkspaceAndTabRejected": True,
            "nativeStopTriggeredDelivery": True,
            "completionConfirmed": False,
        }),
        encoding="utf-8",
    )
    print("MCP_NATIVE_HOOK_HOST_DELIVERY_PASS", flush=True)


def driver() -> None:
    if (
        os.name != "nt"
        or "feat-alera-mcp-control-20261102" not in str(ROOT)
        or not CLI.is_file()
    ):
        raise RuntimeError("requires private Windows Alera MCP worktree + CLI")
    BUILD.mkdir(parents=True, exist_ok=True)
    RESULT.unlink(missing_ok=True)
    outcome: dict = {
        "passed": False, "checks": [], "scope": "disposable",
        "realInteractiveModelReply": False, "nativeHttpNotModelEmitted": True,
    }
    base: Path | None = None
    try:
        with tempfile.TemporaryDirectory(
            prefix="mcp_native_hook_", dir=BUILD, ignore_cleanup_errors=True,
        ) as td:
            base = Path(td)
            runtime = base / "runtime"
            workspace = base / "workspace"
            workspace.mkdir()
            (workspace / "sentinel.txt").write_text("test only\n", encoding="utf-8")
            nonce = uuid.uuid4().hex[:12]
            started = False
            try:
                private_host_start(runtime, base / "private-home")
                started = True
                cli("project", "--runtime-dir", str(runtime), "--json", "add",
                    "--id", PROJECT_ID, "--name", "Private native Hook test",
                    "--repo-path", str(workspace), "--kind", "folder")
                cli("workspace", "--runtime-dir", str(runtime), "--json",
                    "register", "--id", WORKSPACE_ID, "--project-id", PROJECT_ID,
                    "--name", "Private workspace", "--path", str(workspace),
                    "--kind", "main")
                host = HostRpc(runtime)
                try:
                    now = datetime.now(timezone.utc).isoformat(
                        timespec="milliseconds").replace("+00:00", "Z")
                    for handle in (RECEIVER, SENDER):
                        host.request("tab.upsert", {
                            "id": handle, "workspaceId": WORKSPACE_ID,
                            "kind": "terminal", "title": "Private " + handle,
                            "createdAt": now, "updatedAt": now,
                            "payload": {
                                "terminalSessionId": handle,
                                "agentType": "claude", "spawnOnCreate": True,
                            },
                        })
                    # Enable Hook ACCEPTANCE only after both PTYs are launched.
                    # Host startup/install therefore never edits real provider
                    # settings; only this disposable RuntimeStore is changed.
                    host.request("runtimeSettings.update", {
                        "agentStatusHooks": {"claude": True},
                    })
                finally:
                    host.close()
                for _ in range(85):
                    entries = cli("terminal", "--runtime-dir", str(runtime),
                                  "--json", "list", "--workspace",
                                  WORKSPACE_ID).get("items", [])
                    if all(any(t.get("handle") == h and t.get("running")
                               for t in entries) for h in (RECEIVER, SENDER)):
                        break
                    time.sleep(0.2)
                else:
                    raise AssertionError("both real Host-owned PTYs not ready")
                outcome["checks"].append("two_real_private_agent_ptys")
                command = [
                    sys.executable, "-B", "-u", str(Path(__file__).resolve()),
                    "--mode", "receiver", "--runtime", str(runtime),
                    "--workspace", str(workspace), "--state", str(base),
                    "--nonce", nonce,
                ]
                terminal_write(runtime, command, RECEIVER)
                if wait_file(base / "receiver_ready.json", timeout=18,
                             label="receiver PTY ready").get("ready") is not True:
                    raise AssertionError("inert receiver did not start")
                outcome["checks"].append("recipient_safe_input_reader")
                command[command.index("receiver")] = "sender"
                terminal_write(runtime, command, SENDER)
                deadline = time.monotonic() + 65
                while time.monotonic() < deadline:
                    if (base / "sender_error.json").is_file():
                        detail = json.loads((base / "sender_error.json").read_text(
                            encoding="utf-8"))
                        outcome["senderFailurePhase"] = detail.get("phase", "unknown")
                        outcome["senderFailureType"] = detail.get("errorType", "unknown")
                        raise AssertionError("isolated sender failed")
                    if (base / "sender_result.json").is_file():
                        break
                    time.sleep(0.2)
                sender_report = wait_file(base / "sender_result.json", timeout=1,
                                          label="native sender MCP/Hook result")
                if sender_report.get("nativeStopTriggeredDelivery") is not True:
                    raise AssertionError("Host delivery not confirmed")
                outcome["checks"].append("authenticated_native_hook_to_real_pty")
                if wait_file(base / "recipient_received.json", timeout=15,
                             label="actual recipient PTY input").get(
                                 "receivedThroughHostPty") is not True:
                    raise AssertionError("recipient PTY never received the Host paste")
                outcome["checks"].append("host_paste_visible_to_safe_receiver")
                outcome["checks"].append("delivery_not_model_completion")
                outcome["passed"] = True
            finally:
                if started and status(runtime):
                    stop_host(runtime)
        if base is not None:
            for _ in range(25):
                if not base.exists():
                    break
                try:
                    shutil.rmtree(base)
                except OSError:
                    time.sleep(0.3)
            if base.exists():
                raise AssertionError("private runtime cleanup incomplete")
        outcome["cleanupConfirmed"] = True
    except Exception as exc:
        outcome["failureType"] = type(exc).__name__
        outcome["passed"] = False
    RESULT.write_text(json.dumps(outcome, indent=2), encoding="utf-8")
    print("MCP_NATIVE_HOOK_HOST_PTY " +
          ("PASS" if outcome["passed"] else "FAIL") +
          " checks=" + str(len(outcome["checks"])), flush=True)
    if not outcome["passed"]:
        raise AssertionError("private native Hook Host/PTY E2E failed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("driver", "receiver", "sender"), default="driver")
    parser.add_argument("--runtime", type=Path)
    parser.add_argument("--workspace", type=Path)
    parser.add_argument("--state", type=Path)
    parser.add_argument("--nonce")
    args = parser.parse_args()
    if args.mode == "driver":
        driver()
        return
    if not all((args.runtime, args.workspace, args.state, args.nonce)):
        raise RuntimeError("private Agent modes require runtime/workspace/state/nonce")
    if args.mode == "receiver":
        receiver(args.runtime, args.workspace, args.state, args.nonce)
    else:
        try:
            sender(args.runtime, args.workspace, args.state, args.nonce)
        except Exception as error:
            stage_file = args.state / "sender_stage.json"
            phase = "identity_preflight"
            if stage_file.is_file():
                phase = json.loads(stage_file.read_text(encoding="utf-8")).get(
                    "phase", phase)
            (args.state / "sender_error.json").write_text(json.dumps({
                "phase": phase, "errorType": type(error).__name__,
            }), encoding="utf-8")
            raise


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        # Runtime, bearer, prompt and user data must never enter CI logs.
        print("MCP_NATIVE_HOOK_HOST_ERROR " + type(exc).__name__,
              file=sys.stderr, flush=True)
        raise SystemExit(1)
