"""Genuine two-Flutter-Shell MCP window selection acceptance.

Run INSIDE the disposable Alera Host-owned Agent PTY after starting two test
GUI processes (second uses --alera-mcp-e2e-second-shell). A separate fake app
registration is not accepted as evidence. Never touches a normal user profile.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import sys

from mcp_ui_agent_desktop_e2e import HostRpc, call_mcp
from mcp_ui_desktop_fixture import BUILD, RUNTIME, WORKSPACE, WORKSPACE_ID, SESSION_ID

CLI = BUILD / "runner" / "Debug" / "resources" / "alera" / "alera.exe"
REPORT = BUILD / "mcp_two_real_flutter_windows_result.json"
FILE = "src/deep/needle.dart"
SETTING = "editor.tabSize"
ERROR_REPORT = BUILD / "mcp_two_real_flutter_failure.json"
PHASE = "module"
LAST_RESPONSE = {}
PASSED_CHECKS = []


def main() -> None:
    global PHASE, PASSED_CHECKS
    PHASE = "preflight"
    if (os.name != "nt" or "feat-alera-mcp-control-20261102" not in str(BUILD)
            or os.environ.get("ALERA_TERMINAL_HANDLE") != SESSION_ID
            or os.environ.get("ALERA_WORKSPACE_ID") != WORKSPACE_ID
            or len(os.environ.get("ALERA_MCP_UI_TOKEN", "")) != 64
            or not CLI.is_file() or not (WORKSPACE / FILE).is_file()):
        raise RuntimeError("requires the disposable Host-owned Agent PTY")

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


        # Test real cross-window setting propagation with a disposable scalar.
        # All mutations are CAS-guarded, and rollback runs even on failure.
        PHASE = "setting_cas_and_restore"
        original = values[0]
        target = original + 1 if original < 16 else original - 1
        restored = False
        try:
            PHASE = "setting_cas_mutation"
            mutation = mcp("alera.update_setting", {
                "path": SETTING, "expectedValue": original, "value": target,
                "windowId": windows[0],
            }, True)
            if mutation.get("value") != target:
                raise AssertionError("Window A update did not apply requested value")
            passed("real_window_1_setting_cas_applied")

            PHASE = "setting_cas_window_a_readback"
            view_a = mcp("alera.get_setting",
                         {"path": SETTING, "windowId": windows[0]}, True)
            if view_a.get("value") != target:
                portable = host.request("configuration.settings.get", {})
                value = portable.get("editor", {}).get("tabSize")
                LAST_RESPONSE["hostPortableValueMatchesExpected"] = value == target
                LAST_RESPONSE["hostPortableValuePresent"] = isinstance(value, int)
                raise AssertionError("Window A did not read back the applied value")
            passed("real_window_1_setting_readback")

            # The other Flutter process must update independently; no shared
            # in-memory controller is assumed between the two real Shells.
            PHASE = "setting_window_b_observation"
            import time
            deadline = time.monotonic() + 10
            while True:
                view_b = mcp("alera.get_setting",
                             {"path": SETTING, "windowId": windows[1]}, True)
                if view_b.get("value") == target:
                    break
                if time.monotonic() >= deadline:
                    portable = host.request("configuration.settings.get", {})
                    value = portable.get("editor", {}).get("tabSize")
                    LAST_RESPONSE["hostPortableValueMatchesExpected"] = value == target
                    LAST_RESPONSE["hostPortableValuePresent"] = isinstance(value, int)
                    raise AssertionError("Window B did not observe Window A setting update")
                time.sleep(0.35)
            passed("real_window_2_observes_setting_change")
        finally:
            if sys.exc_info()[0] is None:
                PHASE = "setting_original_restoration"
            # Restoration cannot be skipped because an assertion failed.
            view_a = mcp("alera.get_setting",
                         {"path": SETTING, "windowId": windows[0]}, True)
            if view_a.get("value") != original:
                rollback = mcp("alera.update_setting", {
                    "path": SETTING, "expectedValue": view_a["value"],
                    "value": original, "windowId": windows[0],
                }, True)
                if rollback.get("value") != original:
                    raise AssertionError("Could not restore disposable setting")
            restored = mcp("alera.get_setting",
                           {"path": SETTING, "windowId": windows[0]}, True).get("value") == original
            if not restored:
                raise AssertionError("Disposable Window A setting rollback failed")
        passed("disposable_setting_restored")

        PHASE = "result_write"
        REPORT.write_text(json.dumps({
            "passed": len(checks), "checks": checks,
            "windows": len(windows), "realFlutterShells": True,
            "syntheticRegistrations": 0, "disposableProfileOnly": True,
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
        ERROR_REPORT.write_text(json.dumps({
            "phase": PHASE,
            "errorType": type(error).__name__,
            "lastMcpResponse": LAST_RESPONSE,
            "lastPassedCheck": PASSED_CHECKS[-1] if PASSED_CHECKS else None,
            "passedCheckCount": len(PASSED_CHECKS),
            "passed": False,
        }, indent=2), encoding="utf-8")
        print("TWO_REAL_FLUTTER_FAILURE " + type(error).__name__ +
              " phase=" + PHASE, file=sys.stderr, flush=True)
        sys.exit(1)
