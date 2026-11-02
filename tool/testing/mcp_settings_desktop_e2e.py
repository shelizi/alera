"""Disposable real Windows Alera GUI settings MCP E2E.

Runs exclusively from the Host-issued capability-bearing E2E sender PTY.
Checks persistence/read-back, stale CAS denial and settings-dialog navigation.
Restores the original values even if an assertion fails.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sys

from mcp_ui_agent_desktop_e2e import call_mcp
from mcp_agent_desktop_e2e import BUILD, RUNTIME, SENDER_HANDLE, WORKSPACE_ID


def main(cli: Path, report: Path) -> None:
    if os.environ.get("ALERA_TERMINAL_HANDLE") != SENDER_HANDLE:
        raise RuntimeError("Settings E2E requires its own Host-issued sender PTY")
    if os.environ.get("ALERA_WORKSPACE_ID") != WORKSPACE_ID:
        raise RuntimeError("Settings E2E requires the isolated E2E workspace")
    if len(os.environ.get("ALERA_MCP_UI_TOKEN", "")) != 64:
        raise RuntimeError("Host-issued MCP token is missing")
    if not str(cli.resolve()).startswith(str((BUILD / 'runner' / 'Debug').resolve())):
        raise RuntimeError("Only the isolated Windows Debug bundle may run")
    if not str(report.resolve()).startswith(str(BUILD.resolve())):
        raise RuntimeError("Report must stay inside the ignored E2E build tree")

    cases: list[str] = []
    originals: dict[str, object] = {}

    def call(label: str, tool: str, args: dict, ok: bool = True) -> dict:
        response = call_mcp(cli, RUNTIME, tool, args)
        if (response.get("isError") is False) != ok:
            details = (response.get("content") or [{}])[0].get("text", "")
            # Only public MCP error codes, never raw Host paths or secrets.
            if not isinstance(details, str) or len(details) > 120:
                details = "non-public response"
            raise AssertionError(f"{label}: unexpected MCP isError ({details})")
        value = response.get("structuredContent") or {}
        print(f"MCP_SETTINGS_PASS {label}", flush=True)
        cases.append(label)
        return value

    def get(path: str) -> object:
        receipt = call(f"get_{path}", "alera.get_setting", {"path": path})
        if receipt.get("kind") != "setting" or receipt.get("path") != path:
            raise AssertionError(f"{path}: wrong setting read receipt")
        return receipt.get("value")

    def update(label: str, path: str, old: object, value: object) -> object:
        receipt = call(label, "alera.update_setting", {
            "path": path, "expectedValue": old, "value": value,
        })
        if receipt.get("value") != value:
            raise AssertionError(f"{path}: write ACK did not match saved value")
        return receipt.get("value")

    catalog = call("catalog", "alera.list_settings", {"query": "terminal"})
    if not any(item.get("path") == "terminal.fontSize" for item in catalog.get("items", [])):
        raise AssertionError("Terminal font setting missing from typed registry")
    call("unknown_secret_denied", "alera.get_setting", {
        "path": "agents.quotas",
    }, ok=False)
    call("unsafe_setting_mutation_denied", "alera.update_setting", {
        "path": "general.confirmProjectRemoval", "expectedValue": True, "value": False,
    }, ok=False)
    call("invalid_font_range_denied", "alera.update_setting", {
        "path": "terminal.fontSize", "expectedValue": 13, "value": 100,
    }, ok=False)
    safe_values = [
        ("terminal.fontSize", 16),
        ("general.autoGitFetchIntervalMinutes", 6),
    ]
    try:
        for path, next_value in safe_values:
            previous = get(path)
            originals[path] = previous
            if previous == next_value:
                next_value = 15 if path == "terminal.fontSize" else 7
            update(f"update_{path}", path, previous, next_value)
            if get(path) != next_value:
                raise AssertionError(f"{path}: write did not persist to UI owner")
            call(f"stale_compare_denied_{path}", "alera.update_setting", {
                "path": path, "expectedValue": previous,
                "value": next_value,
            }, ok=False)
    finally:
        # Restore exactly and only the two test settings. Never update the
        # regular Alera user profile: the CLI and runtime are fixture-only.
        for path, original in originals.items():
            response = call_mcp(cli, RUNTIME, "alera.get_setting", {"path": path})
            if response.get("isError") is False:
                now = (response.get("structuredContent") or {}).get("value")
                if now != original:
                    undo = call_mcp(cli, RUNTIME, "alera.update_setting", {
                        "path": path, "expectedValue": now, "value": original,
                    })
                    if undo.get("isError") is not False:
                        raise AssertionError(f"Could not restore {path}")
    call("open_real_settings_dialog", "alera.open_settings", {"sectionId": "terminal"})
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps({
        "passed": len(cases), "cases": cases,
        "valuesRestored": True, "workspaceId": WORKSPACE_ID,
        "mode": "isolated Windows Flutter GUI with existing SettingsController",
    }, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"MCP_SETTINGS_SUMMARY {len(cases)} passed", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    options = parser.parse_args()
    try:
        main(options.cli, options.report)
    except Exception as exc:
        if str(options.report.resolve()).startswith(str(BUILD.resolve())):
            options.report.parent.mkdir(parents=True, exist_ok=True)
            options.report.write_text(json.dumps({
                "passed": 0,
                "error": str(exc)[:240],
                "mode": "isolated Windows Flutter settings test",
            }, ensure_ascii=False), encoding="utf-8")
        print(f"MCP_SETTINGS_FAILURE {type(exc).__name__}: {exc}", file=sys.stderr, flush=True)
        raise SystemExit(1)
