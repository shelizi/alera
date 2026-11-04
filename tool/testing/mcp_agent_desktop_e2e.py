"""Step 3: bounded Agent collaboration E2E on the disposable Windows Alera GUI.

Usage: prepare-peer outside the Agent PTY once the isolated App has started;
then run the test from the Host-launched sender Agent Terminal. Never reads or
prints Host-issued capabilities; never targets the normal user's Alera profile.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import sys

from mcp_ui_agent_desktop_e2e import HostRpc, call_mcp
from mcp_ui_desktop_fixture import ROOT, RUNTIME, WORKSPACE_ID


PEER_HANDLE = "alera-mcp-e2e-peer-session"
PEER_TAB = "alera-mcp-e2e-peer-tab"
SENDER_HANDLE = "alera-mcp-e2e-agent-session"
BUILD = ROOT / "build" / "windows" / "shared_gui_ninja"


def prepare_peer() -> None:
    host = HostRpc(RUNTIME)
    try:
        now = datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
        saved = host.request("tab.upsert", {
            "id": PEER_TAB,
            "workspaceId": WORKSPACE_ID,
            "kind": "terminal",
            "title": "MCP E2E Peer Agent",
            "createdAt": now,
            "updatedAt": now,
            "payload": {
                "terminalSessionId": PEER_HANDLE,
                "agentType": "codex",
                "spawnOnCreate": True,
            },
        })
        if saved.get("id") != PEER_TAB:
            raise RuntimeError("Could not create disposable same-workspace peer Terminal")
        host.request("orchestration.agentStatus", {"entries": [
            {
                "terminalSessionId": SENDER_HANDLE,
                "state": "working", "agentType": "codex",
                "prompt": "MCP E2E SENDER TASK",
            },
            {
                "terminalSessionId": PEER_HANDLE,
                "state": "working", "agentType": "codex",
                "prompt": "MCP E2E PEER TASK",
                "toolInput": "MCP_E2E_SECRET_NEVER_EXPOSE",
            },
        ]})
    finally:
        host.close()
    print("MCP_STAGE3_E2E_PEER_READY", flush=True)


def run_agent(cli: Path, result: Path) -> None:
    if not str(result.resolve()).startswith(str(BUILD.resolve())):
        raise RuntimeError("Refusing to write report outside ignored E2E build")
    if not str(cli.resolve()).startswith(str((BUILD / "runner" / "Debug").resolve())):
        raise RuntimeError("CLI must be the bundled executable from this isolated E2E build")
    if os.environ.get("ALERA_TERMINAL_HANDLE") != SENDER_HANDLE:
        raise RuntimeError("E2E must run inside the intended Host-managed sender Terminal")
    if os.environ.get("ALERA_WORKSPACE_ID") != WORKSPACE_ID:
        raise RuntimeError("E2E must run inside its isolated Workspace")
    if len(os.environ.get("ALERA_MCP_UI_TOKEN", "")) != 64:
        raise RuntimeError("Host-minted Agent capability is unavailable")
    tests: list[dict] = []

    def check(label: str, tool: str, args: dict, ok: bool = True) -> dict:
        result = call_mcp(cli, RUNTIME, tool, args)
        if (result.get("isError") is False) != ok:
            raise AssertionError(f"{label}: got unexpected MCP error status")
        data = result.get("structuredContent") or {}
        tests.append({"case": label, "pass": True})
        print("MCP_STAGE3_PASS " + label, flush=True)
        return data

    inventory = check("search_peer", "alera.search_agents", {"query": "MCP E2E PEER"})
    if not any(item.get("handle") == PEER_HANDLE for item in inventory.get("items", [])):
        raise AssertionError("MCP could not find the live peer Agent by prompt")
    if "MCP_E2E_SECRET_NEVER_EXPOSE" in json.dumps(inventory):
        raise AssertionError("Raw tool input leaked from Agent inventory")
    activity = check("read_peer_activity", "alera.get_agent_activity", {"handle": PEER_HANDLE})
    if activity.get("agent", {}).get("agentState") != "working":
        raise AssertionError("The test peer Agent must be busy")
    if "MCP_E2E_SECRET_NEVER_EXPOSE" in json.dumps(activity):
        raise AssertionError("Raw tool input leaked from Agent activity")

    args = {
        "toHandle": PEER_HANDLE,
        "subject": "MCP E2E review",
        "body": "Please review the disposable test change when idle",
        "clientMutationId": "mcp-e2e-status-001",
    }
    sent = check("queue_busy_peer_message", "alera.send_agent_message", args)
    if sent.get("deliveryStatus") != "queued" or sent.get("completionConfirmed") is not False:
        raise AssertionError("Busy Agent message must be queued and never marked complete")
    first_id = sent.get("messageId")
    if not isinstance(first_id, str) or not first_id:
        raise AssertionError("Durable Orchestration receipt has no messageId")
    replay = check("dedupe_message", "alera.send_agent_message", args)
    if replay.get("messageId") != first_id:
        raise AssertionError("Retry inserted a duplicate Agent message")
    changed = dict(args, body="Different payload with reused mutation ID")
    check("reject_idempotency_conflict", "alera.send_agent_message", changed, ok=False)
    status = check("message_delivery_status", "alera.get_agent_message_status", {"messageId": first_id})
    if status.get("deliveryStatus") != "queued" or status.get("completionConfirmed") is not False:
        raise AssertionError("Message status must distinguish queued from completed")
    check("reject_foreign_or_missing_agent", "alera.send_agent_message",
          dict(args, toHandle="foreign-workspace-agent", clientMutationId="mcp-e2e-status-002"), ok=False)
    check("reject_missing_agent_focus", "alera.focus_agent", {"handle": "missing-agent"}, ok=False)

    focused = check("focus_peer_real_flutter_tab", "alera.focus_agent", {"handle": PEER_HANDLE})
    if focused.get("status") != "applied" or focused.get("tabId") != PEER_TAB:
        raise AssertionError("Workbench did not visibly select the existing peer Agent Terminal")
    again = check("focus_peer_no_new_tab", "alera.focus_agent", {"handle": PEER_HANDLE})
    if again.get("tabId") != PEER_TAB:
        raise AssertionError("Repeated Agent focus changed the terminal Tab")
    result.parent.mkdir(parents=True, exist_ok=True)
    result.write_text(json.dumps({
        "passed": len(tests), "cases": tests,
        "workspaceId": WORKSPACE_ID,
        "messageId": first_id,
        "deliveryStatus": "queued",
        "uiTabId": PEER_TAB,
        "mode": "real Alera GUI, test-only synthetic Agent presence",
    }, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"MCP_STAGE3_SUMMARY {len(tests)} passed", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("prepare-peer", "run"))
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--result", type=Path)
    opts = parser.parse_args()
    if "feat-alera-mcp-control-20261102" not in str(ROOT):
        raise RuntimeError("Refusing to run outside the pinned isolated MCP worktree")
    try:
        if opts.mode == "prepare-peer":
            prepare_peer()
        elif opts.cli is not None and opts.result is not None:
            run_agent(opts.cli, opts.result)
        else:
            parser.error("run requires --cli and --result")
    except Exception as error:
        print(f"MCP_STAGE3_FAIL {type(error).__name__}: {error}", file=sys.stderr, flush=True)
        sys.exit(1)
