"""Isolated real-PTY Agent queue→idle→delivered test.

The Agent-status transition is synthetic, representing a completed status
hook; the PTY write/deferred Enter and persisted delivered_at are real Host
operations. This is NOT a test with a real running Codex/Claude model.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sys
import time

from mcp_ui_agent_desktop_e2e import HostRpc, call_mcp
from mcp_agent_desktop_e2e import BUILD, RUNTIME, PEER_HANDLE, SENDER_HANDLE, WORKSPACE_ID


def main(cli: Path, report: Path) -> None:
    if os.environ.get("ALERA_TERMINAL_HANDLE") != SENDER_HANDLE:
        raise RuntimeError("Must run from the isolated real sender PTY")
    if os.environ.get("ALERA_WORKSPACE_ID") != WORKSPACE_ID:
        raise RuntimeError("Wrong isolated workspace")
    if not str(cli.resolve()).startswith(str((BUILD / 'runner' / 'Debug').resolve())):
        raise RuntimeError("Refusing non-isolated CLI")
    if not str(report.resolve()).startswith(str(BUILD.resolve())):
        raise RuntimeError("Refusing report outside disposable build")
    cases: list[str] = []

    def mcp(name: str, args: dict) -> dict:
        response = call_mcp(cli, RUNTIME, name, args)
        if response.get("isError") is not False:
            raise AssertionError(f"MCP {name} failed")
        return response.get("structuredContent") or {}

    host = HostRpc(RUNTIME)
    try:
        host.request("orchestration.agentStatus", {"entries": [{
            "terminalSessionId": PEER_HANDLE,
            "agentType": "codex", "state": "working",
            "prompt": "Disposable Agent is busy",
        }]})
        receipt = mcp("alera.send_agent_message", {
            "toHandle": PEER_HANDLE,
            "subject": "Isolated delivery transition",
            "body": "Please acknowledge this disposable fixture message",
            "clientMutationId": "mcp-delivery-e2e-001",
        })
        if receipt.get("deliveryStatus") != "queued":
            raise AssertionError("Busy peer was not queued")
        cases.append("busy_queued_without_enter")
        print("MCP_DELIVERY_PASS busy_queued_without_enter", flush=True)

        message_id = receipt["messageId"]
        status = mcp("alera.get_agent_message_status", {"messageId": message_id})
        if status.get("deliveryStatus") != "queued":
            raise AssertionError("Busy peer was incorrectly marked delivered")
        cases.append("busy_status_unconfirmed")
        print("MCP_DELIVERY_PASS busy_status_unconfirmed", flush=True)

        # Trigger the actual AgentPresence hook path. This will schedule Host
        # PTY paste and deferred Enter; we DO NOT set delivered_at manually.
        host.request("orchestration.agentStatus", {"entries": [{
            "terminalSessionId": PEER_HANDLE,
            "agentType": "codex", "state": "done",
            "prompt": "Disposable Agent is ready",
        }]})
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            status = mcp("alera.get_agent_message_status", {"messageId": message_id})
            if status.get("deliveryStatus") == "delivered":
                break
            time.sleep(0.25)
        else:
            raise AssertionError("Real PTY deferred Enter never confirmed delivery")
        cases.append("real_pty_idle_delivery_confirmed")
        print("MCP_DELIVERY_PASS real_pty_idle_delivery_confirmed", flush=True)
        if status.get("completionConfirmed") is not False:
            raise AssertionError("Delivery incorrectly reported task completion")
        cases.append("delivery_not_completion")
        print("MCP_DELIVERY_PASS delivery_not_completion", flush=True)

        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text(json.dumps({
            "passed": len(cases), "cases": cases,
            "status": "delivered", "completionConfirmed": False,
            "transition": "synthetic Agent done status, actual Host PTY delivery ACK",
        }, ensure_ascii=False, indent=2), encoding="utf-8")
        print(f"MCP_DELIVERY_SUMMARY {len(cases)} passed", flush=True)
    finally:
        host.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    opts = parser.parse_args()
    try:
        main(opts.cli, opts.report)
    except Exception as exc:
        print(f"MCP_DELIVERY_FAILURE {type(exc).__name__}: {exc}", file=sys.stderr, flush=True)
        raise SystemExit(1)
