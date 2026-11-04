"""Run Alera MCP UI requests from an actual Agent PTY in an isolated desktop fixture.

This intentionally runs *inside* the Host-spawned Agent terminal, so the
Host-generated ALERA_MCP_UI_TOKEN is inherited rather than forged or copied
from database/configuration. The token and Host credential are never logged.

Only point --runtime, --cli and --result to a disposable E2E worktree.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time


def control_file(runtime: Path) -> Path:
    for candidate in ("host.json", "runtime-host.json"):
        found = runtime / candidate
        if found.exists():
            return found
    raise RuntimeError("Isolated Runtime Host control file was not found")


class HostRpc:
    def __init__(self, runtime: Path, client_kind: str = "cli") -> None:
        control = json.loads(control_file(runtime).read_text(encoding="utf-8"))
        self.sock = socket.create_connection(("127.0.0.1", int(control["port"])), timeout=6)
        self.sock.settimeout(12)
        self.reader = self.sock.makefile("r", encoding="utf-8", newline="\n")
        self.writer = self.sock.makefile("w", encoding="utf-8", newline="\n")
        self.next_id = 0
        self.request(
            "hello",
            {
                "protocolVersion": control["protocolVersion"],
                "token": control["token"],
                "clientKind": client_kind,
            },
        )

    def request(self, name: str, payload: dict) -> dict:
        self.next_id += 1
        req_id = self.next_id
        self.writer.write(json.dumps({"id": req_id, "type": name, "payload": payload}) + "\n")
        self.writer.flush()
        while True:
            line = self.reader.readline()
            if not line:
                raise RuntimeError(f"Host disconnected during {name}")
            result = json.loads(line)
            if result.get("id") != req_id:
                continue  # The Host is also allowed to send asynchronous UI events.
            if not result.get("ok"):
                raise RuntimeError(f"Host rejected {name}: {str(result.get('error', 'error'))[:220]}")
            return result.get("payload") or {}

    def close(self) -> None:
        self.writer.close()
        self.reader.close()
        self.sock.close()


def call_mcp(cli: Path, runtime: Path, name: str, args: dict) -> dict:
    request = {"jsonrpc": "2.0", "id": 8, "method": "tools/call",
               "params": {"name": name, "arguments": args}}
    completed = subprocess.run(
        [str(cli), "mcp", "--runtime-dir", str(runtime), "serve"],
        input=json.dumps(request, separators=(",", ":")) + "\n",
        encoding="utf-8",
        capture_output=True,
        timeout=28,
        check=False,
    )
    if completed.returncode:
        raise RuntimeError(f"MCP process returned exit {completed.returncode}")
    lines = [line for line in completed.stdout.splitlines() if line.strip()]
    if len(lines) != 1:
        raise RuntimeError(f"Expected one MCP JSON-RPC response, got {len(lines)}")
    result = json.loads(lines[0])
    if result.get("id") != 8 or not isinstance(result.get("result"), dict):
        raise RuntimeError("Invalid MCP response ID or result")
    return result["result"]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--result", type=Path, required=True)
    parser.add_argument("--focus", choices=("git_graph", "branch_compare"))
    opts = parser.parse_args()

    runtime = opts.runtime.resolve()
    cli = opts.cli.resolve()
    result_file = opts.result.resolve()
    if "shared_gui_ninja" not in str(runtime) or "e2e_support" not in str(runtime):
        raise RuntimeError("Refusing non-disposable Runtime Host path")
    if "shared_gui_ninja" not in str(result_file):
        raise RuntimeError("Refusing result file outside isolated build")
    if len(os.environ.get("ALERA_MCP_UI_TOKEN", "")) != 64:
        raise RuntimeError("Not running in a Host-managed Agent PTY")
    workspace = os.environ.get("ALERA_WORKSPACE_ID")
    session = os.environ.get("ALERA_TERMINAL_HANDLE")
    if not workspace or not session:
        raise RuntimeError("Missing live PTY identity")

    # Targeted visible-view replay for collecting exact-window screenshots.
    # Uses the same real PTY capability and success contract as the suite.
    if opts.focus:
        name, args = {
            "git_graph": (
                "alera.open_git_graph",
                {"gitDiffRoot": "git_fixture", "allBranches": True},
            ),
            "branch_compare": (
                "alera.compare_branches",
                {"gitDiffRoot": "git_fixture", "baseRef": "main",
                 "headRef": "feature/demo"},
            ),
        }[opts.focus]
        response = call_mcp(cli, runtime, name, args)
        value = response.get("structuredContent") or {}
        if response.get("isError") is not False or value.get("status") != "applied":
            raise AssertionError(f"{opts.focus}: Flutter did not confirm applied")
        print(f"E2E_FOCUS_APPLIED {opts.focus} tabId={value.get('tabId')}", flush=True)
        return 0

    cases: list[dict] = []
    def check(label: str, name: str, args: dict, *, success: bool) -> dict:
        response = call_mcp(cli, runtime, name, args)
        actual_success = response.get("isError") is False
        if actual_success != success:
            detail = response.get("content") or []
            raise AssertionError(f"{label}: expected success={success}, got {str(detail)[:180]}")
        value = response.get("structuredContent") or {}
        if success and (value.get("status") != "applied" or value.get("workspaceId") != workspace):
            raise AssertionError(f"{label}: UI did not actually confirm applied")
        cases.append({"case": label, "pass": True, "applied": success,
                      "tabId": value.get("tabId") if success else None})
        print(f"E2E_PASS {label}", flush=True)
        return value

    host = HostRpc(runtime)
    shadow: HostRpc | None = None
    try:
        identity = {"sessionId": session, "workspaceId": workspace,
                    "capability": os.environ["ALERA_MCP_UI_TOKEN"]}
        windows = host.request("uiControl.windows", identity).get("windows", [])
        if len(windows) != 1:
            raise AssertionError(f"Expected one real desktop Shell, got {len(windows)}")
        window_id = windows[0]

        opened = check("open_file", "alera.open_file", {"relativePath": "src/deep/needle.dart"}, success=True)
        reopened = check("reuse_file_tab", "alera.open_file", {"relativePath": "src/deep/needle.dart"}, success=True)
        if opened.get("tabId") != reopened.get("tabId"):
            raise AssertionError("Opening the same file created a duplicate Tab")
        check("explorer_reveal", "alera.reveal_file", {"relativePath": "src/deep/needle.dart"}, success=True)
        graph = check("git_graph", "alera.open_git_graph",
                      {"gitDiffRoot": "git_fixture", "allBranches": True}, success=True)
        compare = check("branch_compare", "alera.compare_branches",
                        {"gitDiffRoot": "git_fixture", "baseRef": "main",
                         "headRef": "feature/demo"}, success=True)
        if graph.get("tabId") == compare.get("tabId"):
            raise AssertionError("Graph and branch compare unexpectedly share a Tab")

        check("missing_file", "alera.open_file", {"relativePath": "src/deep/absent.dart"}, success=False)
        check("traversal_denied", "alera.open_file", {"relativePath": "../secret"}, success=False)
        check("invalid_ref_denied", "alera.compare_branches",
              {"gitDiffRoot": "git_fixture", "baseRef": "--bad", "headRef": "main"}, success=False)
        check("unknown_window_denied", "alera.open_file",
              {"relativePath": "src/deep/needle.dart", "windowId": "not-a-real-window"}, success=False)

        # This connection is deliberately a *test-only* authenticated App
        # registration; it has no Flutter UI. It tests Host target selection
        # and two-window ambiguity without touching another user's windows.
        shadow = HostRpc(runtime, client_kind="app")
        shadow.request("uiControl.register", {"windowId": "mcp-e2e-shadow-window"})
        check("ambiguous_window_denied", "alera.open_file",
              {"relativePath": "src/deep/needle.dart"}, success=False)
        check("explicit_real_window", "alera.open_file",
              {"relativePath": "src/deep/needle.dart", "windowId": window_id}, success=True)
        shadow.request("uiControl.unregister", {"windowId": "mcp-e2e-shadow-window"})
        shadow.close()
        shadow = None

        check("auto_window_restored", "alera.open_file",
              {"relativePath": "src/deep/needle.dart"}, success=True)
        result_file.parent.mkdir(parents=True, exist_ok=True)
        result_file.write_text(json.dumps(
            {"passed": len(cases), "cases": cases, "workspaceId": workspace,
             "windowRouting": "one real Flutter window + shadow Host client"},
            indent=2, ensure_ascii=False), encoding="utf-8")
        print(f"E2E_SUMMARY {len(cases)} passed", flush=True)
    finally:
        if shadow is not None:
            try:
                shadow.request("uiControl.unregister", {"windowId": "mcp-e2e-shadow-window"})
            except Exception:
                pass
            shadow.close()
        host.close()
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print(f"E2E_FAILURE {type(error).__name__}: {error}", file=sys.stderr, flush=True)
        sys.exit(1)
