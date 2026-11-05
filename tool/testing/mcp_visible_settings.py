"""Read-only native evidence for the two-process Flutter acceptance test."""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import time

SETTING = "editor.tabSize"


def native_snapshot(bundle: Path) -> dict:
    probe = Path(__file__).with_name("mcp_visible_settings.ps1")
    result = subprocess.run(
        ["powershell.exe", "-NoProfile", "-NonInteractive", "-File", str(probe),
         "-Bundle", str(bundle)],
        capture_output=True, text=True, timeout=8, check=False,
    )
    if result.returncode:
        # Native exceptions can include local paths; never put them in reports.
        raise AssertionError("native Settings inspection unavailable")
    return json.loads(result.stdout)


def native_windows(data: dict) -> list[dict]:
    windows = data.get("windows", [])
    if (len(windows) != 2
            or len({w["pid"] for w in windows}) != 2
            or len({w["hwnd"] for w in windows}) != 2
            or not all(w["pid"] > 0 and w["hwnd"] > 0 for w in windows)):
        raise AssertionError("requires two distinct native Flutter processes/windows")
    return windows


def visible_editor(data: dict, expected: str, *, identity=None) -> tuple[int, int]:
    windows = native_windows(data)
    editors = [(w, f) for w in windows for f in w["fields"]]
    if len(editors) != 1:
        raise AssertionError("requires one uniquely identified native Tab Size field")
    window, field = editors[0]
    actual = (window["pid"], window["hwnd"])
    if (not window["visible"] or window["minimized"]
            or not window["foreground"] or field["offscreen"]
            or not field["contained"] or field["value"] != expected
            or (identity is not None and identity != actual)):
        raise AssertionError("expected Tab Size is not visible in the same native window")
    return actual


def await_visible(snapshot, value, identity=None, *, timeout=10):
    deadline = time.monotonic() + timeout
    while True:
        try:
            return visible_editor(snapshot(), str(value), identity=identity)
        except AssertionError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.2)


def observe_change(mcp, snapshot, windows, original, identity, *, timeout=10,
                   evidence=None):
    """No commands to B after its baseline; even failed ACKs trigger rollback."""
    target = original + 1 if original < 8 else original - 1
    if evidence is None:
        evidence = {}
    evidence.update({
        "nativeProcessId": identity[0], "nativeWindowHandle": identity[1],
        "baselineValue": original, "targetValue": target,
        "targetVisible": False, "rollbackCommitted": False,
        "rollbackVisible": False,
    })
    try:
        mutation = mcp("alera.update_setting", {
            "path": SETTING, "expectedValue": original, "value": target,
            "windowId": windows[0],
        }, True)
        if mutation.get("value") != target:
            raise AssertionError("Window A CAS ACK did not match requested value")
        await_visible(snapshot, target, identity, timeout=timeout)
        evidence["targetVisible"] = True
    finally:
        current = mcp("alera.get_setting", {
            "path": SETTING, "windowId": windows[0],
        }, True).get("value")
        # Never overwrite an unrelated concurrent edit, even in the fixture.
        if type(current) is not int or current not in (original, target):
            raise AssertionError("rollback refused unexpected concurrent setting value")
        if current != original:
            undo = mcp("alera.update_setting", {
                "path": SETTING, "expectedValue": target, "value": original,
                "windowId": windows[0],
            }, True)
            if undo.get("value") != original:
                raise AssertionError("disposable setting rollback ACK failed")
        evidence["rollbackCommitted"] = True
        await_visible(snapshot, original, identity, timeout=timeout)
        evidence["rollbackVisible"] = True
