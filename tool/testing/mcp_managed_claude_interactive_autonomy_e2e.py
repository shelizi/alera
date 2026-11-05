"""Explicit-only DIRECT interactive Claude native Hook -> private Host -> PTY.

Legacy filename: this does NOT exercise production agentProfile.launch or
Managed Profile MCP injection. Claude itself owns stdin for both turns.
Default invocation is read-only preflight. A real run requires
--run-isolated-model and an already provisioned isolated --provider-home.
Never approves trust, copies credentials or uses the developer's account home.
No prompts, PTY contents, credentials or bearer values are logged or saved.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import uuid

from mcp_real_model_collaboration_e2e import (
    BUILD, CLI, PROJECT_ID, WORKSPACE_ID, SENDER, RECEIVER,
    cli, status, stop_host, terminal_write, wait_file,
)
from mcp_ui_agent_desktop_e2e import HostRpc, call_mcp
from mcp_native_hook_host_delivery_e2e import private_host_start
from mcp_managed_profile_acceptance import isolated_provider_home

ROOT = Path(__file__).resolve().parents[2]
RESULT = BUILD / "mcp_direct_claude_interactive_result.json"


def wait_or_fail(state: Path, filename: str, *, timeout: float) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for role in ("recipient", "sender"):
            failed = state / (role + "_failure.json")
            if failed.is_file():
                info = json.loads(failed.read_text(encoding="utf-8"))
                raise AssertionError(
                    "private " + role + " failed at " +
                    str(info.get("phase", "unknown")) + " (" +
                    str(info.get("errorType", "unknown")) + ")"
                )
        target = state / filename
        if target.is_file():
            return json.loads(target.read_text(encoding="utf-8"))
        time.sleep(0.2)
    raise AssertionError("private test stage timed out: " + filename)


def set_phase(state: Path, role: str, name: str) -> None:
    (state / (role + "_phase.json")).write_text(
        json.dumps({"phase": name}), encoding="utf-8"
    )


def private_pty(handle: str, runtime: Path, workspace: Path) -> None:
    if (
        os.name != "nt"
        or not runtime.resolve().is_relative_to(BUILD.resolve())
        or workspace.resolve().parent != runtime.resolve().parent
        or os.environ.get("ALERA_TERMINAL_HANDLE") != handle
        or os.environ.get("ALERA_WORKSPACE_ID") != WORKSPACE_ID
        or len(os.environ.get("ALERA_MCP_UI_TOKEN", "")) != 64
        or not (workspace / "sentinel.txt").is_file()
        or not (runtime / "runtime.sqlite").is_file()
    ):
        raise RuntimeError("must run inside the isolated Host-minted Agent PTY")


def make_private_claude_hook_settings(state: Path) -> Path:
    """One-invocation settings only: actual Alera Rust hook relay, no user hooks."""
    hook_dir = state / "per-launch-hooks"
    hook_dir.mkdir(exist_ok=True)
    hooks: dict[str, list[dict]] = {}
    for event in ("UserPromptSubmit", "Stop"):
        script = hook_dir / (event.lower() + ".cmd")
        script.write_text(
            "@echo off\r\nsetlocal\r\n"
            "set \"ALERA_AGENT_TYPE=claude\"\r\n"
            "set \"ALERA_AGENT_HOOK_EVENT=" + event + "\"\r\n"
            "echo " + event + ">>\"" + str(state / "hook_event_names.log") + "\"\r\n"
            "\"" + str(CLI.resolve()) + "\" agent-hook-relay\r\n"
            "exit /b 0\r\n",
            encoding="utf-8",
        )
        # Same cmd/Git Bash dispatch shape as Alera integration_config.
        cmd = "MSYS2_ARG_CONV_EXCL='*' cmd.exe /d /s /c call " + shlex.quote(str(script))
        hooks[event] = [{"hooks": [{"type": "command", "command": cmd}]}]
    target = state / "private_claude_hooks.json"
    target.write_text(json.dumps({"hooks": hooks}), encoding="utf-8")
    return target


def recipient(runtime: Path, workspace: Path, state: Path, nonce: str) -> None:
    """Real interactive Claude TUI owns the Host PTY stdin for both turns.

    Never approves workspace trust or permissions. Unlike the model --print
    adapter, the Python runner does not read/deliver Agent prompts itself.
    """
    private_pty(RECEIVER, runtime, workspace)
    set_phase(state, "recipient", "verify_private_hook_settings")
    settings = state / "private_claude_hooks.json"
    account_file = state / "provider_home.txt"
    if not settings.is_file() or not account_file.is_file():
        raise AssertionError("private Claude settings/account-home path unavailable")
    provider_home = account_file.read_text(encoding="utf-8")
    if not isolated_provider_home(provider_home):
        raise AssertionError("preprovisioned isolated provider HOME required")
    claude = shutil.which("claude.exe")
    if not claude:
        raise AssertionError("Claude Code native executable missing")
    env = os.environ.copy()
    env["USERPROFILE"] = provider_home
    env["HOME"] = provider_home
    env.pop("CLAUDE_CONFIG_DIR", None)
    endpoint = runtime / "agent-hooks" / "endpoint.cmd"
    if not endpoint.is_file():
        raise AssertionError("isolated Alera Host native Hook endpoint missing")
    for line in endpoint.read_text(encoding="utf-8").splitlines():
        if line.startswith("set ALERA_AGENT_HOOK_") and "=" in line:
            key, value = line[4:].split("=", 1)
            env[key] = value.strip()
    if not env.get("ALERA_AGENT_HOOK_TOKEN") or not env.get("ALERA_AGENT_HOOK_PORT"):
        raise AssertionError("private Host native Hook bearer unavailable")
    prompt = (
        "Isolated read-only Alera Agent integration. Reply only with the "
        "uppercase first letters of Green Meadow Sun. Remain available for "
        "another message afterward. Do not use any tools, edit files or "
        "inspect the workspace."
    )
    args = [
        claude, "--setting-sources", "", "--settings", str(settings),
        "--strict-mcp-config", "--tools", "", "--restricted",
        "--disable-slash-commands", "--no-chrome",
        "--permission-mode", "plan", "--effort", "low",
        "--", prompt,
    ]
    (state / "model_ready.json").write_text('{"ready":true}', encoding="utf-8")
    set_phase(state, "recipient", "interactive_claude_tui")
    # Attach the real interactive provider directly to its Host-owned ConPTY.
    # No Python adapter consumes or resubmits the queued Agent-to-Agent prompt.
    proc = subprocess.run(
        args, cwd=str(workspace), env=env,
        stdin=None, stdout=None, stderr=None, timeout=190,
    )
    if proc.returncode:
        raise AssertionError("interactive Claude exited unsuccessfully")
    raise AssertionError("interactive Claude terminated before second turn")


def sender(runtime: Path, workspace: Path, state: Path, nonce: str) -> None:
    private_pty(SENDER, runtime, workspace)
    def mcp(name: str, args: dict) -> dict:
        value = call_mcp(CLI, runtime, name, args)
        if value.get("isError") is not False:
            raise AssertionError("private Host-issued MCP request was rejected")
        return value.get("structuredContent") or {}
    set_phase(state, "sender", "wait_native_model_working")
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        observed = call_mcp(CLI, runtime, "alera.get_agent_activity", {
            "handle": RECEIVER,
        })
        agent = (observed.get("structuredContent") or {}).get("agent") or {}
        if observed.get("isError") is False and agent.get("agentState") == "working":
            break
        time.sleep(0.2)
    else:
        raise AssertionError("real Claude UserPromptSubmit did not mark Agent working")
    (state / "sender_seen_working.json").write_text(
        '{"realProviderHookWorking":true}', encoding="utf-8")
    set_phase(state, "sender", "queue_mcp_message")
    receipt = mcp("alera.send_agent_message", {
        "toHandle": RECEIVER,
        "subject": "Private actual model Hook delivery",
        "body": "Reply ONLY with the reversed uppercase identifier: " + nonce +
                ". No tools, file access or edits.",
        "clientMutationId": "model-native-hook-" + nonce,
    })
    if receipt.get("deliveryStatus") != "queued":
        raise AssertionError("Host did not persist message as queued")
    msg_id = receipt.get("messageId")
    if not isinstance(msg_id, str):
        raise AssertionError("no durable messageId")
    (state / "sender_queued.json").write_text('{"queued":true}', encoding="utf-8")
    set_phase(state, "sender", "wait_native_host_delivery")
    deadline = time.monotonic() + 105
    while time.monotonic() < deadline:
        data = mcp("alera.get_agent_message_status", {"messageId": msg_id})
        if data.get("deliveryStatus") == "delivered":
            if data.get("completionConfirmed") is not False:
                raise AssertionError("Host confused delivery and completion")
            (state / "sender_result.json").write_text(
                '{"nativeModelHookTriggeredHostDelivery":true,'
                '"completionConfirmed":false}', encoding="utf-8")
            return
        time.sleep(0.3)
    raise AssertionError("real Claude native Stop never triggered Host delivery")


def driver(provider_home: Path, *, allow_model: bool = False) -> None:
    # Validate before touching reports, starting a Host or invoking a provider.
    if not allow_model or not isolated_provider_home(provider_home):
        raise RuntimeError("explicit isolated model run and private account required")
    if (os.name != "nt" or not CLI.is_file() or not shutil.which("claude.exe")
            or "dropbox" in [part.lower() for part in BUILD.resolve().parts]):
        raise RuntimeError("requires disposable Windows Alera Host + Claude Code")
    original_home = str(provider_home.resolve())
    RESULT.unlink(missing_ok=True)
    summary: dict = {
        "passed": False, "checks": [], "privateHostOnly": True,
        "providerEmitsNativeHook": False, "autonomousInteractiveModelReply": False,
        "launchPath": "direct_provider_in_host_pty",
        "productionManagedProfileAccepted": False,
    }
    BUILD.mkdir(parents=True, exist_ok=True)
    base: Path | None = None
    try:
        with tempfile.TemporaryDirectory(
            prefix="mcp_rc_", dir=BUILD,
            ignore_cleanup_errors=True,
        ) as td:
            base = Path(td)
            runtime, workspace = base / "runtime", base / "workspace"
            workspace.mkdir()
            (workspace / "sentinel.txt").write_text("Private Alera Hook test.\n", encoding="utf-8")
            nonce = uuid.uuid4().hex[:12]
            # Private Host startup, but keep the AUTH HOME VALUE available to
            # the model child only; do not copy credentials or override Host.
            # private_host_start is already guarded to remove provider homes.
            env_home = original_home
            process_stopped = False
            started = False
            try:
                summary["phase"] = "start_private_host"
                private_host_start(runtime, base / "private-home")
                started = True
                summary["phase"] = "register_private_project"
                cli("project", "--runtime-dir", str(runtime), "--json",
                    "add", "--id", PROJECT_ID, "--name",
                    "Private Claude native Hook test",
                    "--repo-path", str(workspace), "--kind", "folder")
                summary["phase"] = "register_private_workspace"
                cli("workspace", "--runtime-dir", str(runtime), "--json",
                    "register", "--id", WORKSPACE_ID, "--project-id", PROJECT_ID,
                    "--name", "Private", "--path", str(workspace), "--kind", "main")
                summary["phase"] = "connect_private_host"
                h = HostRpc(runtime)
                try:
                    now = datetime.now(timezone.utc).isoformat(
                        timespec="milliseconds").replace("+00:00", "Z")
                    summary["phase"] = "spawn_private_host_ptys"
                    for handle in (RECEIVER, SENDER):
                        h.request("tab.upsert", {
                            "id": handle, "workspaceId": WORKSPACE_ID,
                            "kind": "terminal", "title": "Private " + handle,
                            "createdAt": now, "updatedAt": now,
                            "payload": {
                                "terminalSessionId": handle,
                                "agentType": "claude",
                                "spawnOnCreate": True,
                            },
                        })
                    # This changes only the disposable RuntimeStore. The
                    # PTYs were already spawned while Hook installation was
                    # disabled, so no global Claude/Codex config is touched.
                    summary["phase"] = "enable_private_hook_receiver"
                    h.request("runtimeSettings.update", {
                        "agentStatusHooks": {"claude": True},
                    })
                finally:
                    h.close()
                summary["phase"] = "wait_private_ptys"
                for _ in range(75):
                    entries = cli("terminal", "--runtime-dir", str(runtime),
                                  "--json", "list", "--workspace",
                                  WORKSPACE_ID).get("items", [])
                    if all(any(x.get("handle") == handle and x.get("running")
                               for x in entries) for handle in (RECEIVER, SENDER)):
                        break
                    time.sleep(0.2)
                else:
                    raise AssertionError("both private Agent PTYs did not start")
                summary["checks"].append("two_private_host_agent_ptys")
                make_private_claude_hook_settings(base)
                # Deliver account-home via a path-only temp file rather than a
                # PTY command argument; receiver never persists or prints it.
                (base / "provider_home.txt").write_text(env_home, encoding="utf-8")
                cmd = [
                    sys.executable, "-B", "-u", str(Path(__file__).resolve()),
                    "--mode", "recipient", "--runtime", str(runtime),
                    "--workspace", str(workspace), "--state", str(base),
                    "--nonce", nonce,
                ]
                summary["phase"] = "launch_private_recipient"
                terminal_write(runtime, cmd, RECEIVER)
                wait_or_fail(base, "model_ready.json", timeout=20)
                summary["checks"].append("private_per_process_hook_settings")
                cmd[cmd.index("recipient")] = "sender"
                summary["phase"] = "launch_private_sender"
                terminal_write(runtime, cmd, SENDER)
                wait_or_fail(base, "sender_seen_working.json", timeout=55)
                summary["checks"].append("model_hook_marked_host_working")
                wait_or_fail(base, "sender_queued.json", timeout=25)
                summary["checks"].append("scoped_mcp_message_queued")
                summary["phase"] = "wait_first_real_hook_delivery"
                wait_or_fail(base, "sender_result.json", timeout=110)
                summary["checks"].append("native_model_stop_triggered_host")
                summary["phase"] = "wait_second_interactive_model_response"
                expected_reply = nonce[::-1].upper()
                deadline = time.monotonic() + 115
                second_turn = False
                while time.monotonic() < deadline:
                    events_file = base / "hook_event_names.log"
                    events = events_file.read_text(encoding="utf-8").splitlines() \
                        if events_file.is_file() else []
                    text = cli(
                        "terminal", "--runtime-dir", str(runtime), "--json",
                        "read", "--handle", RECEIVER, "--max-bytes", "131072",
                    ).get("text", "")
                    terminal_text = re.sub(
                        r"\x1b\[[0-?]*[ -/]*[@-~]", "", str(text),
                    )
                    if (events.count("UserPromptSubmit") >= 2
                            and events.count("Stop") >= 2
                            and expected_reply in re.sub(r"\s+", "", terminal_text)):
                        second_turn = True
                        break
                    time.sleep(0.4)
                if not second_turn:
                    raise AssertionError("real interactive Claude did not reply after Host PTY delivery")
                summary["checks"].append("real_interactive_claude_second_turn")
                summary["checks"].append("native_hook_two_completed_turns")
                summary["passed"] = True
                summary["providerEmitsNativeHook"] = True
                summary["autonomousInteractiveModelReply"] = True
                summary["phase"] = "private_host_shutdown"
            finally:
                if started and status(runtime):
                    stop_host(runtime)
                    process_stopped = True
            summary["privateHostShutdownConfirmed"] = process_stopped
        summary["phase"] = "private_files_cleanup"
        # Real interactive Windows Claude may finish its ConPTY child process
        # slightly after Host shutdown. Do not claim full PASS or abandon
        # credentials-containing private folders until every file is gone.
        for _ in range(100):
            if not base or not base.exists():
                break
            try:
                shutil.rmtree(base)
            except OSError:
                time.sleep(0.3)
        summary["privateFilesRemoved"] = not base or not base.exists()
        if not summary["privateFilesRemoved"]:
            raise AssertionError("private Host files not cleaned")
    except Exception as error:
        summary["passed"] = False
        summary["failedAt"] = summary.get("phase", "preflight")
        summary["failureType"] = type(error).__name__
        hint = str(error)
        if hint.startswith(("private recipient failed at ",
                            "private sender failed at ",
                            "private test stage timed out: ")):
            # Only report fixed phase names and exception classes from this
            # disposable harness, never raw Host/PTY/provider error messages.
            summary["failureHint"] = hint[:180]
    RESULT.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print("MCP_DIRECT_CLAUDE_INTERACTIVE " +
          ("PASS" if summary["passed"] else "FAIL") +
          " checks=" + str(len(summary["checks"])), flush=True)
    if not summary["passed"]:
        raise AssertionError("real model native Hook Host E2E failed")


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--mode", choices=("driver", "sender", "recipient"), default="driver")
    p.add_argument("--runtime", type=Path)
    p.add_argument("--workspace", type=Path)
    p.add_argument("--state", type=Path)
    p.add_argument("--nonce")
    p.add_argument("--provider-home", type=Path)
    p.add_argument("--run-isolated-model", action="store_true")
    a = p.parse_args()
    if a.mode == "driver":
        if not a.run_isolated_model:
            print(json.dumps({
                "scope": "direct Claude launch; not production MANAGED acceptance",
                "liveRunExecutedByThisCommand": False,
                "isolatedProviderHomeReady": isolated_provider_home(a.provider_home),
                "sidecarAvailable": CLI.is_file(),
                "productionManagedProfileAccepted": False,
            }))
            return
        driver(a.provider_home, allow_model=True)
        return
    try:
        if not all((a.runtime, a.workspace, a.state, a.nonce)):
            raise RuntimeError("private PTY modes need runtime/workspace/state/nonce")
        if a.mode == "sender":
            sender(a.runtime, a.workspace, a.state, a.nonce)
        else:
            recipient(a.runtime, a.workspace, a.state, a.nonce)
    except Exception as error:
        if a.state is not None and a.state.is_dir():
            statefile = a.state / (a.mode + "_phase.json")
            phase = "initialization"
            if statefile.is_file():
                phase = json.loads(statefile.read_text(encoding="utf-8")).get(
                    "phase", phase
                )
            (a.state / (a.mode + "_failure.json")).write_text(json.dumps({
                "phase": phase, "errorType": type(error).__name__,
            }), encoding="utf-8")
        raise


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print("MCP_REAL_CLAUDE_NATIVE_HOST_FAILURE " + type(error).__name__,
              file=sys.stderr, flush=True)
        raise SystemExit(1)
