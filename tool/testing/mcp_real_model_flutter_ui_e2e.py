"""Real Claude model -> MCP -> Host -> Alera Flutter file Tab applied ACK.

This runs ONLY inside the disposable Host-managed Windows desktop E2E Agent
PTY. It allows exactly one named MCP UI operation against a harmless test
file and verifies the correlated model tool_result; prose is never accepted
as proof. No auth tokens or model transcript are logged or persisted.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from mcp_ui_desktop_fixture import BUILD, RUNTIME, WORKSPACE, WORKSPACE_ID, SESSION_ID
from mcp_ui_agent_desktop_e2e import call_mcp

SERVER = "alera-workbench"
TOOL = "alera_open_file"
PATH = "src/deep/needle.dart"
CLI = BUILD / "runner" / "Debug" / "resources" / "alera" / "alera.exe"
REPORT = BUILD / "mcp_real_model_flutter_ui_result.json"


def main() -> int:
    if os.name != "nt" or "feat-alera-mcp-control-20261102" not in str(BUILD):
        raise RuntimeError("requires the pinned isolated Windows Alera MCP worktree")
    if os.environ.get("ALERA_TERMINAL_HANDLE") != SESSION_ID:
        raise RuntimeError("not inside the disposable Host-managed Agent terminal")
    if os.environ.get("ALERA_WORKSPACE_ID") != WORKSPACE_ID:
        raise RuntimeError("wrong disposable workspace")
    if len(os.environ.get("ALERA_MCP_UI_TOKEN", "")) != 64:
        raise RuntimeError("missing Host-minted per-Agent UI permission")
    if not (WORKSPACE / PATH).is_file() or not CLI.is_file():
        raise RuntimeError("test-only file or bundled Alera CLI is missing")
    if "--preflight" in sys.argv:
        result = call_mcp(CLI, RUNTIME, "alera.open_file", {"relativePath": PATH})
        content = json.dumps(result.get("content", []), ensure_ascii=False).lower()
        codes = [
            "ui_not_ready", "workspace_missing", "invalid_path",
            "unauthorized", "ui_disconnected", "ui_operation_failed",
            "no unique workbench window", "in time",
        ]
        error_code = next((code for code in codes if code in content), "other")
        success = result.get("isError") is False
        print("DIRECT_FLUTTER_UI_PREFLIGHT=" + str(success)
              + " ERROR_CATEGORY=" + ("none" if success else error_code), flush=True)
        return 0 if success else 1
    claude = shutil.which("claude.exe")
    if not claude:
        raise RuntimeError("Claude Code CLI is missing")

    config = json.dumps({
        "mcpServers": {
            SERVER: {
                "command": str(CLI.resolve()),
                "args": ["mcp", "--runtime-dir", str(RUNTIME.resolve()), "serve"],
            },
        },
    }, separators=(",", ":"))
    prompt = (
        "Use the Alera MCP tool mcp__alera-workbench__alera_open_file with "
        'arguments {"relativePath":"src/deep/needle.dart"} exactly once. '
        "Only open this existing test file in the Alera UI; do not edit any "
        "content, change settings, run shell commands or call another tool. "
        "Only if the actual MCP tool result says status applied, respond "
        "ALERA_FLUTTER_UI_APPLIED. Otherwise answer ALERA_FLUTTER_UI_FAILED."
    )
    args = [
        claude, "--no-session-persistence",
        "--strict-mcp-config", "--mcp-config", config,
        "--setting-sources", "", "--tools", "",
        "--allowedTools", f"mcp__{SERVER}__{TOOL}",
        "--max-turns", "3", "--max-budget-usd", "0.10",
        "--output-format", "stream-json", "--verbose",
        "--print", prompt,
    ]
    started = time.monotonic()
    try:
        response = subprocess.run(
            args, cwd=str(WORKSPACE), capture_output=True,
            text=True, encoding="utf-8", errors="replace", timeout=95,
            stdin=subprocess.DEVNULL,
        )
    except subprocess.TimeoutExpired:
        raise RuntimeError("bounded real model test timed out")

    use_ids: set[str] = set()
    completed: list[bool] = []
    tool_error_categories = []
    tool_result_shapes = []
    for line in response.stdout.splitlines():
        try:
            event = json.loads(line)
        except (ValueError, TypeError):
            continue
        if not isinstance(event, dict):
            continue
        msg = event.get("message")
        if not isinstance(msg, dict) or not isinstance(msg.get("content"), list):
            continue
        if event.get("type") == "assistant":
            for block in msg["content"]:
                if not isinstance(block, dict) or block.get("type") != "tool_use":
                    continue
                if block.get("name") == f"mcp__{SERVER}__{TOOL}":
                    if block.get("input", {}).get("relativePath") == PATH:
                        identifier = block.get("id")
                        if isinstance(identifier, str):
                            use_ids.add(identifier)
        if event.get("type") == "user":
            for block in msg["content"]:
                if (not isinstance(block, dict)
                        or block.get("type") != "tool_result"
                        or block.get("tool_use_id") not in use_ids):
                    continue
                raw_content = block.get("content", "")
                content = (raw_content if isinstance(raw_content, str)
                           else json.dumps(raw_content, ensure_ascii=False))
                tool_result_shapes.append({
                    "kind": type(block.get("content")).__name__,
                    "size": len(content),
                    "hasApplied": "applied" in content.lower(),
                    "hasTabId": "tabid" in content.lower(),
                    "hasError": "error" in content.lower(),
                    "isError": block.get("is_error") is True,
                })
                if block.get("is_error"):
                    hints = [
                        "workspace_missing", "invalid_path", "unauthorized",
                        "ui_not_ready", "ui_disconnected", "ui_operation_failed",
                        "permission", "denied", "failed",
                    ]
                    tool_error_categories.append(next(
                        (hint for hint in hints if hint in content.lower()),
                        "unknown",
                    ))
                completed.append(
                    block.get("is_error") is not True
                    # Claude's stream-json tool_result content is already
                    # a JSON string. Re-encoding it as JSON escapes the
                    # quotation marks and caused a false test failure.
                    # The content is a correlated *real tool_result*, never
                    # an assistant chat message or final prose.
                    and '"status"' in content
                    and '"applied"' in content
                    and '"tabId"' in content
                )

    passed = response.returncode == 0 and len(use_ids) == 1 and completed == [True]
    summary = {
        "status": "pass" if passed else "fail",
        "model": "Claude Code",
        "actualModelToolCalls": len(use_ids),
        "correlatedFlutterAppliedAcks": completed.count(True),
        "toolErrors": completed.count(False),
        "toolErrorCategories": tool_error_categories[:4],
        "toolResultShapes": tool_result_shapes[:3],
        "modelExitCode": response.returncode,
        "elapsedSeconds": round(time.monotonic() - started, 1),
        "operation": "open existing disposable Flutter file tab",
        "userProjectModified": False,
    }
    REPORT.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(
        "REAL_MODEL_FLUTTER_UI "
        + summary["status"]
        + " toolCalls=" + str(len(use_ids))
        + " appliedAcks=" + str(completed.count(True)),
        flush=True,
    )
    return 0 if passed else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print("REAL_MODEL_FLUTTER_UI_ERROR " + type(error).__name__, file=sys.stderr)
        sys.exit(1)
