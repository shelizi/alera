"""Invoke an authenticated real Codex/Claude model from a disposable Alera PTY.

Never prints/persists inherited Host-issued MCP capabilities or provider
credentials. Requires the exact private Host-issued Terminal identity.
Evidence must include an actual structured model MCP tool call; the model's
final claim alone is insufficient. One read-only call, no edits or shell.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build" / "windows" / "shared_gui_ninja"
BASE = BUILD / "e2e_host_restart"
RUNTIME = BASE / "runtime"
WORKSPACE = BASE / "workspace"
CLI = BUILD / "runner" / "Debug" / "resources" / "alera" / "alera.exe"
HANDLE = "mcp-real-restart-agent-session"
WORKSPACE_ID = "mcp-real-restart-isolated-workspace"
SERVER = "alera-workbench"
TOOL = "alera.list_workspaces"


def classify_failure(text: str) -> str:
    lower = text.lower()
    if any(word in lower for word in ("unauthorized", "not logged in", "authentication")):
        return "provider_auth"
    if any(word in lower for word in ("rate limit", "usage limit", "quota", "429")):
        return "provider_quota"
    if any(word in lower for word in ("network", "connection", "timeout", "dns", "proxy")):
        return "provider_connectivity"
    if any(word in lower for word in ("invalid config", "unknown option", "unrecognized")):
        return "provider_cli_options"
    return "provider_or_mcp_error"


def check_pty() -> None:
    if os.name != "nt" or "feat-alera-mcp-control-20261102" not in str(ROOT):
        raise RuntimeError("only the pinned disposable Windows Alera worktree is allowed")
    if os.environ.get("ALERA_TERMINAL_HANDLE") != HANDLE:
        raise RuntimeError("must run inside the Host-owned disposable test Agent PTY")
    if os.environ.get("ALERA_WORKSPACE_ID") != WORKSPACE_ID:
        raise RuntimeError("Agent is not bound to the private test workspace")
    if len(os.environ.get("ALERA_MCP_UI_TOKEN", "")) != 64:
        raise RuntimeError("Host-issued Agent MCP capability is not present")
    if not CLI.is_file() or not (WORKSPACE / "sentinel.txt").is_file():
        raise RuntimeError("disposable MCP executable or sentinel is missing")
    if not (RUNTIME / "runtime.sqlite").is_file():
        raise RuntimeError("disposable Runtime Host is missing")


def event_is_mcp(event: dict) -> bool:
    item = event.get("item")
    if isinstance(item, dict):
        return (item.get("type") == "mcp_tool_call"
                and item.get("server") == SERVER
                and item.get("tool") == TOOL)
    msg = event.get("message")
    if isinstance(msg, dict):
        return any(
            block.get("type") == "tool_use"
            and TOOL.replace(".", "_") in str(block.get("name", ""))
            for block in msg.get("content", []) if isinstance(block, dict)
        )
    return False


def run_model(provider: str) -> dict:
    tool_args = ["mcp", "--runtime-dir", str(RUNTIME.resolve()), "serve"]
    command = str(CLI.resolve())
    prompt = (
        "Read-only Alera MCP integration test. You MUST invoke the actual MCP "
        "server alera-workbench and its tool alera.list_workspaces exactly once. "
        "No shell commands, no file edits, no other tools. From the real tool "
        "result, find workspace ID 'mcp-real-restart-isolated-workspace'. "
        "If present, answer only ALERA_REAL_MCP_OK; otherwise answer "
        "ALERA_REAL_MCP_MISSING. Do not claim success without the tool."
    )
    env = os.environ.copy()
    if provider == "codex":
        executable = shutil.which("codex.cmd")
        if executable is None:
            raise RuntimeError("Codex CLI missing")
        args = [
            executable,
            "--config", f'mcp_servers.{SERVER}.command={json.dumps(command)}',
            "--config", f'mcp_servers.{SERVER}.args={json.dumps(tool_args)}',
            "exec", "--json", "--ephemeral", "--ignore-user-config",
            "--sandbox", "read-only", "--skip-git-repo-check",
            "--cd", str(WORKSPACE.resolve()), prompt,
        ]
    elif provider == "claude":
        executable = shutil.which("claude.exe")
        if executable is None:
            raise RuntimeError("Claude CLI missing")
        config = json.dumps({
            "mcpServers": {SERVER: {"command": command, "args": tool_args}},
        }, separators=(",", ":"))
        args = [
            # --bare disables OAuth and keychain credentials on this installed
            # Claude version; the user is already signed in with OAuth.
            executable, "--no-session-persistence",
            "--strict-mcp-config", "--mcp-config", config,
            "--setting-sources", "", "--tools", "",
            "--allowedTools", "mcp__alera-workbench__alera_list_workspaces",
            "--max-turns", "3", "--max-budget-usd", "0.10",
            "--output-format", "stream-json", "--verbose",
            "--print", prompt,
        ]
    else:
        raise RuntimeError("unknown model provider")

    started = time.monotonic()
    try:
        completed = subprocess.run(
            args, cwd=str(WORKSPACE), env=env, capture_output=True,
            text=True, encoding="utf-8", errors="replace",
            timeout=95, stdin=subprocess.DEVNULL,
        )
    except subprocess.TimeoutExpired:
        return {"provider": provider, "status": "timeout",
                "toolCalls": 0, "durationSec": round(time.monotonic() - started, 1)}

    found = []
    claude_tool_ids: set[str] = set()
    claude_matching_results = 0
    claude_workspace_from_tool = False
    event_types = []
    result_kinds = []
    claude_init = []
    claude_blocks = []
    claude_result_clues = []
    claude_user_event_shapes = []
    for line in completed.stdout.splitlines():
        try:
            event = json.loads(line)
        except (ValueError, TypeError):
            continue
        if isinstance(event, dict):
            event_types.append(str(event.get("type", ""))[:40])
            if provider == "claude" and event.get("type") == "system":
                servers = event.get("mcp_servers")
                claude_init.append({
                    "subtype": str(event.get("subtype", ""))[:40],
                    "serverStates": [
                        {"name": str(server.get("name", ""))[:50],
                         "status": str(server.get("status", ""))[:40]}
                        for server in servers if isinstance(server, dict)
                    ] if isinstance(servers, list) else [],
                    "mcpToolNames": [
                        str(name)[:100] for name in event.get("tools", [])
                        if isinstance(name, str) and "alera" in name.lower()
                    ][:20],
                })
            if provider == "claude" and event.get("type") == "assistant":
                msg = event.get("message")
                if isinstance(msg, dict):
                    for block in msg.get("content", []):
                        if (isinstance(block, dict) and block.get("type") == "tool_use"
                                and TOOL.replace(".", "_") in str(block.get("name", ""))):
                            identifier = block.get("id")
                            if isinstance(identifier, str):
                                claude_tool_ids.add(identifier)
                    claude_blocks.extend([
                        {"type": str(block.get("type", ""))[:40],
                         "tool": str(block.get("name", ""))[:100]}
                        for block in msg.get("content", [])
                        if isinstance(block, dict)
                    ])
            if provider == "claude" and event.get("type") == "user":
                msg = event.get("message")
                claude_user_event_shapes.append({
                    "messageObject": isinstance(msg, dict),
                    "messageKeys": sorted(msg.keys())[:8] if isinstance(msg, dict) else [],
                    "blocks": [
                        {"type": str(block.get("type", ""))[:35],
                         "idMatches": block.get("tool_use_id") in claude_tool_ids,
                         "isError": bool(block.get("is_error", False)),
                         "contentHasWorkspace": WORKSPACE_ID in json.dumps(
                             block.get("content", ""), ensure_ascii=False),
                        }
                        for block in msg.get("content", [])
                        if isinstance(block, dict)
                    ][:8] if isinstance(msg, dict) and isinstance(msg.get("content"), list) else [],
                    "hasTopLevelToolUseResult": "tool_use_result" in event,
                })
                if isinstance(msg, dict):
                    for block in msg.get("content", []):
                        if (isinstance(block, dict) and block.get("type") == "tool_result"
                                and block.get("tool_use_id") in claude_tool_ids
                                and not block.get("is_error", False)):
                            claude_matching_results += 1
                            # Only the result for the exact observed tool_use
                            # is evidence; the final prose never counts.
                            if WORKSPACE_ID in json.dumps(block.get("content", ""), ensure_ascii=False):
                                claude_workspace_from_tool = True
            if event.get("type") == "result":
                result_kinds.append(str(event.get("subtype", ""))[:50])
                if provider == "claude":
                    output = str(event.get("result", ""))[:600]
                    claude_result_clues.append({
                        "isError": event.get("is_error"),
                        "toolMissingMarker": "ALERA_REAL_MCP_MISSING" in output,
                        "toolOkMarker": "ALERA_REAL_MCP_OK" in output,
                        "permission": "permission" in output.lower(),
                        "unavailable": "unavailable" in output.lower(),
                    })
        if isinstance(event, dict) and event_is_mcp(event):
            found.append(event)

    mcp_called = bool(found)
    # Verify real tool output, not only a model's prose response.
    structured = json.dumps(found, ensure_ascii=False)
    found_workspace = (claude_workspace_from_tool if provider == "claude"
                       else WORKSPACE_ID in structured)
    statuses = []
    for event in found:
        item = event.get("item")
        if isinstance(item, dict):
            statuses.append(str(item.get("status", "")))
    succeeded = (
        completed.returncode == 0 and mcp_called and found_workspace
        and not any(status in ("failed", "error") for status in statuses)
    )
    report = {
        "provider": provider,
        "status": "pass" if succeeded else "fail",
        "exitCode": completed.returncode,
        "mcpToolCallObserved": mcp_called,
        "expectedWorkspaceObservedInToolEvent": found_workspace,
        "mcpEventCount": len(found),
        "correlatedToolResults": claude_matching_results if provider == "claude" else len(found),
        "mcpStatuses": statuses[:5],
        "durationSec": round(time.monotonic() - started, 1),
    }
    if not succeeded:
        report["failureClass"] = classify_failure(completed.stderr + completed.stdout[-1600:])
        if provider == "claude":
            report["eventTypes"] = event_types[-12:]
            report["resultKinds"] = result_kinds[:4]
            report["init"] = claude_init[:3]
            report["blocks"] = claude_blocks[:10]
            report["resultClues"] = claude_result_clues[:2]
            report["userEventShapes"] = claude_user_event_shapes[:3]
            # Only bounded, redacted command diagnostics; never store the
            # streamed model transcript, bearer capability or CLI auth.
            preview = completed.stderr.strip().splitlines()[-3:]
            preview = " | ".join(preview)[:260]
            preview = re.sub(r"(?i)sk-[a-z0-9_-]{12,}", "[secret]", preview)
            preview = re.sub(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+", "[email]", preview)
            preview = re.sub(r"[a-fA-F0-9]{40,}", "[opaque]", preview)
            report["cliErrorHint"] = preview
    return report


def main(provider: str) -> int:
    check_pty()
    report = run_model(provider)
    target = BASE / f"mcp_real_model_{provider}_result.json"
    target.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print("REAL_MODEL_MCP_RESULT provider=" + provider
          + " status=" + report["status"]
          + " invoked=" + str(report.get("mcpToolCallObserved", False))
          + " workspace=" + str(report.get("expectedWorkspaceObservedInToolEvent", False))
          + " errorClass=" + str(report.get("failureClass", "none")), flush=True)
    return 0 if report["status"] == "pass" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--provider", choices=("codex", "claude"), required=True)
    opts = parser.parse_args()
    try:
        raise SystemExit(main(opts.provider))
    except Exception as exc:
        print("REAL_MODEL_MCP_ERROR " + type(exc).__name__ + " "
              + str(exc)[:120], file=sys.stderr, flush=True)
        raise SystemExit(1)
