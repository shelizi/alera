"""Real Claude model -> Alera MCP -> Flutter settings owner -> verified rollback.

Run ONLY from a disposable Host-owned Agent PTY in the isolated Windows E2E
runtime. The model may use *one* allowlisted scalar-setting mutation tool.
The test itself first reads the current value and always restores it via a
fresh compare-and-set, including when a model call fails. It never modifies
the normal user's Alera settings, global MCP config or file contents.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from mcp_ui_agent_desktop_e2e import call_mcp
from mcp_ui_desktop_fixture import BUILD, RUNTIME, WORKSPACE, WORKSPACE_ID, SESSION_ID

SERVER = "alera-workbench"
TOOL_NAME = f"mcp__{SERVER}__alera_update_setting"
SETTING = "editor.tabSize"
CLI = BUILD / "runner" / "Debug" / "resources" / "alera" / "alera.exe"
REPORT = BUILD / "mcp_real_model_settings_result.json"


def setting() -> int:
    reply = call_mcp(CLI, RUNTIME, "alera.get_setting", {"path": SETTING})
    data = reply.get("structuredContent") or {}
    value = data.get("value")
    if (reply.get("isError") is not False or data.get("path") != SETTING
            or data.get("status") != "applied"
            or isinstance(value, bool) or not isinstance(value, int)
            or not 1 <= value <= 16):
        raise AssertionError("private Flutter setting read was not applied")
    return value


def change(previous: int, next_value: int) -> None:
    reply = call_mcp(CLI, RUNTIME, "alera.update_setting", {
        "path": SETTING, "expectedValue": previous, "value": next_value,
    })
    data = reply.get("structuredContent") or {}
    if (reply.get("isError") is not False or data.get("status") != "applied"
            or data.get("path") != SETTING or data.get("value") != next_value):
        raise AssertionError("private Flutter setting update was not confirmed")


def real_model_update(previous: int, next_value: int) -> dict:
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
        "Alera MCP *disposable test workspace* setting acceptance. "
        "Only call the exact tool mcp__alera-workbench__alera_update_setting "
        "ONCE with these exact arguments: "
        + json.dumps({"path": SETTING, "expectedValue": previous,
                      "value": next_value}, separators=(",", ":")) + ". "
        "Do not use shell, file tools, other MCP tools, or modify anything else. "
        "Only report success if the tool itself returns status applied."
    )
    arguments = [
        claude, "--no-session-persistence",
        "--strict-mcp-config", "--mcp-config", config,
        "--setting-sources", "", "--tools", "",
        "--allowedTools", TOOL_NAME,
        "--max-turns", "3", "--max-budget-usd", "0.10",
        "--output-format", "stream-json", "--verbose", "--print", prompt,
    ]
    started = time.monotonic()
    try:
        process = subprocess.run(
            arguments, cwd=str(WORKSPACE), capture_output=True, text=True,
            encoding="utf-8", errors="replace", timeout=95,
            stdin=subprocess.DEVNULL,
        )
    except subprocess.TimeoutExpired as error:
        raise AssertionError("bounded model setting tool call timed out") from error
    used: set[str] = set()
    returns: list[bool] = []
    for line in process.stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        if not isinstance(event, dict):
            continue
        message = event.get("message")
        if not isinstance(message, dict) or not isinstance(message.get("content"), list):
            continue
        if event.get("type") == "assistant":
            for block in message["content"]:
                if (isinstance(block, dict) and block.get("type") == "tool_use"
                        and block.get("name") == TOOL_NAME
                        and block.get("input") == {
                            "path": SETTING, "expectedValue": previous,
                            "value": next_value,
                        } and isinstance(block.get("id"), str)):
                    used.add(block["id"])
        elif event.get("type") == "user":
            for block in message["content"]:
                if (not isinstance(block, dict) or block.get("type") != "tool_result"
                        or block.get("tool_use_id") not in used):
                    continue
                content = block.get("content")
                if isinstance(content, str):
                    try:
                        body = json.loads(content)
                    except ValueError:
                        body = {}
                else:
                    body = content
                # Claude sometimes wraps the Alera MCP JSON response in text
                # blocks rather than exposing it as a parsed object.
                if isinstance(body, list):
                    text = " ".join(
                        item.get("text", "") for item in body
                        if isinstance(item, dict) and isinstance(item.get("text"), str)
                    )
                    try:
                        body = json.loads(text)
                    except ValueError:
                        body = {}
                if not isinstance(body, dict):
                    body = {}
                returns.append(
                    block.get("is_error") is not True
                    and body.get("kind") == "setting"
                    and body.get("path") == SETTING
                    and body.get("status") == "applied"
                    and body.get("value") == next_value
                )
    return {
        "modelExitCode": process.returncode,
        "modelIssuedExactUpdateCalls": len(used),
        "correlatedAppliedReceipts": returns.count(True),
        "correlatedFailedReceipts": returns.count(False),
        "durationSec": round(time.monotonic() - started, 1),
    }


def main() -> None:
    if (os.name != "nt" or "feat-alera-mcp-control-20261102" not in str(BUILD)
            or os.environ.get("ALERA_TERMINAL_HANDLE") != SESSION_ID
            or os.environ.get("ALERA_WORKSPACE_ID") != WORKSPACE_ID
            or len(os.environ.get("ALERA_MCP_UI_TOKEN", "")) != 64
            or not CLI.is_file() or not (WORKSPACE / "src/deep/needle.dart").is_file()):
        raise RuntimeError("real settings test requires its own isolated Alera Agent PTY")

    original = setting()
    target = original + 1 if original < 16 else 15
    evidence: dict = {}
    restored = False
    try:
        evidence = real_model_update(original, target)
        evidence["readbackMatches"] = setting() == target
    finally:
        # A model refusal, timeout or failed assertion must never leave the
        # private test setting changed; CAS uses its actual latest value.
        now = setting()
        if now != original:
            change(now, original)
        restored = setting() == original
    passed = (
        evidence.get("modelExitCode") == 0
        and evidence.get("modelIssuedExactUpdateCalls") == 1
        and evidence.get("correlatedAppliedReceipts") == 1
        and evidence.get("correlatedFailedReceipts") == 0
        and evidence.get("readbackMatches") is True
        and restored
    )
    report = {
        "passed": passed,
        "setting": SETTING,
        "modelToolCall": evidence,
        "originalSettingRestored": restored,
        "workspace": "disposable_Alera_test_only",
    }
    REPORT.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(
        "REAL_MODEL_SETTING_RESULT status=" + ("PASS" if passed else "FAIL")
        + " applied=" + str(evidence.get("correlatedAppliedReceipts", 0))
        + " readback=" + str(evidence.get("readbackMatches", False))
        + " restored=" + str(restored),
        flush=True,
    )
    if not passed:
        raise AssertionError("real model's setting change was not fully acknowledged and restored")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print("REAL_MODEL_SETTING_ERROR " + type(exc).__name__, file=sys.stderr, flush=True)
        sys.exit(1)
