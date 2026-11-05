"""Genuine two-Flutter-Shell MCP window selection acceptance.

Run INSIDE the disposable Alera Host-owned Agent PTY after starting two test
GUI processes (second uses --alera-mcp-e2e-second-shell). A separate fake app
registration is not accepted as evidence. Never touches a normal user profile.

Keep window B foreground for native accessibility inspection. The test opens
its Editor Settings once before CAS; afterward it only observes UI Automation.
Missing/ambiguous native field evidence fails closed, including during rollback.
"""
from __future__ import annotations

import base64
import json
import os
from pathlib import Path
import sys

from mcp_ui_agent_desktop_e2e import HostRpc, call_mcp
from mcp_ui_desktop_fixture import BUILD, RUNTIME, WORKSPACE, WORKSPACE_ID, SESSION_ID
from mcp_visible_settings import (
    await_visible, native_snapshot, native_windows, observe_change,
)

CLI = BUILD / "runner" / "Debug" / "resources" / "alera" / "alera.exe"
REPORT = BUILD / "mcp_two_real_flutter_windows_result.json"
FILE = "src/deep/needle.dart"
SETTING = "editor.tabSize"
ERROR_REPORT = BUILD / "mcp_two_real_flutter_failure.json"
PHASE = "module"
LAST_RESPONSE = {}
PASSED_CHECKS = []
NATIVE_EVIDENCE = {}


def main() -> None:
    global PHASE, PASSED_CHECKS
    PHASE = "preflight"
    REPORT.unlink(missing_ok=True)
    ERROR_REPORT.unlink(missing_ok=True)
    if (os.name != "nt"
            or os.environ.get("ALERA_TERMINAL_HANDLE") != SESSION_ID
            or os.environ.get("ALERA_WORKSPACE_ID") != WORKSPACE_ID
            or len(os.environ.get("ALERA_MCP_UI_TOKEN", "")) != 64
            or not CLI.is_file() or not (WORKSPACE / FILE).is_file()):
        raise RuntimeError("requires the disposable Host-owned Agent PTY")
    # Pin isolation to this checkout's generated Flutter configuration rather
    # than the name of an older feature worktree.
    root = Path(__file__).resolve().parents[2]
    config = root / "windows/flutter/ephemeral/generated_config.cmake"
    support_define = "ALERA_E2E_SUPPORT_DIR=" + (BUILD / "e2e_support").as_posix()
    token = base64.b64encode(support_define.encode()).decode()
    if not config.is_file() or token not in config.read_text(encoding="utf-8"):
        raise RuntimeError("current checkout disposable support define is missing")

    checks: list[str] = []
    PASSED_CHECKS = checks
    def passed(name: str) -> None:
        checks.append(name)
        print("TWO_REAL_FLUTTER_PASS " + name, flush=True)

    def mcp(name: str, args: dict, expected_success: bool) -> dict:
        global LAST_RESPONSE
        response = call_mcp(CLI, RUNTIME, name, args)
        value = response.get("structuredContent") or {}
        public_error_codes = (
            "workspace_missing", "ui_not_ready", "not_visible", "ui_timeout",
            "ui_disconnected", "ui_operation_failed", "file_missing",
            "invalid_path", "unauthorized", "window_not_found",
            "no_registered_window", "failed_to_apply", "ui_busy",
        )
        # Never persist raw MCP result text, because it may contain paths or
        # user data. This list only exposes fixed public error-code labels.
        error_text = json.dumps(response.get("content") or [], ensure_ascii=True)
        LAST_RESPONSE = {
            "tool": name, "expectedSuccess": expected_success,
            "actualSuccess": response.get("isError") is False,
            "flutterApplied": value.get("status") == "applied",
            "hasWorkspaceId": bool(value.get("workspaceId")),
            "hasTabId": bool(value.get("tabId")),
            "publicErrorCodes": [c for c in public_error_codes if c in error_text],
        }
        if (response.get("isError") is False) != expected_success:
            raise AssertionError(f"{name}: success={expected_success} mismatch")
        value = response.get("structuredContent") or {}
        if expected_success and value.get("status") != "applied":
            raise AssertionError(f"{name}: Flutter did not confirm applied")
        return value

    PHASE = "connect_host"
    host = HostRpc(RUNTIME)
    try:
        PHASE = "window_registry"
        identity = {
            "sessionId": SESSION_ID,
            "workspaceId": WORKSPACE_ID,
            "capability": os.environ["ALERA_MCP_UI_TOKEN"],
        }
        # A native HWND can appear before post-bootstrap UI registration.
        # Wait for TWO real Host-registered Shell identities; no fake client.
        import time
        windows = []
        until = time.monotonic() + 20
        while time.monotonic() < until:
            windows = host.request("uiControl.windows", identity).get("windows", [])
            if (len(windows) == 2
                    and all(isinstance(w, str) and w for w in windows)
                    and windows[0] != windows[1]):
                break
            time.sleep(0.25)
        else:
            raise AssertionError("two real Flutter Shells not registered before deadline")
        passed("two_distinct_live_flutter_shell_registrations")

        # A cheap read-only ACK probes the Flutter command bus before any
        # file/workspace navigation. A Host window registration alone is not
        # proof the Flutter event listener can complete UI commands.
        PHASE = "first_window_setting_ack_probe"
        mcp("alera.get_setting",
            {"path": SETTING, "windowId": windows[0]}, True)
        passed("first_window_setting_ack_before_navigation")

        PHASE = "ambiguous_target"
        mcp("alera.open_file", {"relativePath": FILE}, False)
        passed("unqualified_route_rejected_when_two_windows")

        PHASE = "explicit_file_target"
        ids: list[str] = []
        for index, window_id in enumerate(windows, 1):
            PHASE = "explicit_file_target_" + str(index)
            opened = mcp("alera.open_file",
                         {"relativePath": FILE, "windowId": window_id}, True)
            if opened.get("workspaceId") != WORKSPACE_ID or not opened.get("tabId"):
                raise AssertionError("open_file ACK had wrong workspace/tab")
            ids.append(opened["tabId"])
            passed(f"real_window_{index}_editor_applied")

        PHASE = "reuse_file_tab"
        if ids[0] != ids[1]:
            raise AssertionError("Both Shells did not reuse the existing file tab")
        passed("two_shells_reuse_one_persisted_tab")

        mcp("alera.open_file",
            {"relativePath": FILE, "windowId": "not-a-registered-shell"}, False)
        passed("forged_window_id_rejected")

        PHASE = "setting_read"
        values = []
        for index, window_id in enumerate(windows, 1):
            value = mcp("alera.get_setting",
                        {"path": SETTING, "windowId": window_id}, True)
            if (value.get("path") != SETTING or isinstance(value.get("value"), bool)
                    or not isinstance(value.get("value"), int)):
                raise AssertionError("Window-specific setting read invalid")
            values.append(value["value"])
            passed(f"real_window_{index}_setting_read_applied")
        if values[0] != values[1]:
            raise AssertionError("Two Shells began with different setting values")
        passed("two_shells_consistent_initial_setting")


        # Establish B's native identity causally: neither process has an
        # Editor pane before the command addressed exclusively to B. Only B
        # may expose the unique visible field afterward.
        PHASE = "native_settings_baseline"
        bundle = BUILD / "runner" / "Debug" / "alera-dev.exe"
        snapshot = lambda: native_snapshot(bundle)
        before = native_windows(snapshot())
        if any(window["fields"] for window in before):
            raise AssertionError("close existing Editor Settings panes before acceptance")
        original = values[0]
        if not 1 <= original <= 8:
            raise AssertionError("unexpected disposable editor tab-size range")
        mcp("alera.open_settings", {
            "sectionId": "editor", "windowId": windows[1],
        }, True)
        identity_b = await_visible(snapshot, original)
        if identity_b not in {(w["pid"], w["hwnd"]) for w in before}:
            raise AssertionError("native B was replaced while opening Settings")
        passed("real_window_2_native_editor_field_visible_before_cas")

        PHASE = "native_setting_cas_observation_and_rollback"
        observe_change(mcp, snapshot, windows, original, identity_b,
                       evidence=NATIVE_EVIDENCE)
        passed("real_window_2_visible_field_updated_without_mcp_read")
        passed("disposable_setting_restored_and_visible_in_window_2")

        PHASE = "result_write"
        REPORT.write_text(json.dumps({
            "passed": len(checks), "checks": checks,
            "windows": len(windows), "realFlutterShells": True,
            "syntheticRegistrations": 0, "disposableProfileOnly": True,
            "visibleSettingsEvidence": "native_windows_ui_automation",
            "nativeSettingsObservation": NATIVE_EVIDENCE,
            "windowBAfterCasMcpReads": 0, "visibleRollbackVerified": True,
        }, indent=2), encoding="utf-8")
        print(f"TWO_REAL_FLUTTER_SUMMARY {len(checks)} passed", flush=True)
    finally:
        host.close()


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # The log is restricted to a fixed phase and exception class. Never
        # persist the prompt, raw exception, token, PTY output or model data.
        failure = json.dumps({
            "phase": PHASE,
            "errorType": type(error).__name__,
            "lastMcpResponse": LAST_RESPONSE,
            "lastPassedCheck": PASSED_CHECKS[-1] if PASSED_CHECKS else None,
            "passedCheckCount": len(PASSED_CHECKS),
            "nativeSettingsObservation": NATIVE_EVIDENCE,
            "passed": False,
        }, indent=2)
        if ERROR_REPORT.parent.is_dir():
            ERROR_REPORT.write_text(failure, encoding="utf-8")
        print("TWO_REAL_FLUTTER_FAILURE " + type(error).__name__ +
              " phase=" + PHASE, file=sys.stderr, flush=True)
        sys.exit(1)
