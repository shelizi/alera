"""Inspect actual Windows Alera Managed Agent Profile MCP auto-injection.

The fixture is already running in a private Runtime Host, started with
ALERA_MCP_PROFILE_INJECTION=1. This script does not launch, trust, approve,
type into, or terminate any Agent. It only checks live OS process arguments,
Host-owned PTY identity and MCP child process presence. No command lines,
model conversations, PTY contents, or credentials are printed or persisted.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess

from mcp_real_host_restart_e2e import BUILD, RUNTIME, WORKSPACE_ID


def live_sessions(cli: pathlib.Path) -> set[str]:
    completed = subprocess.run(
        [str(cli), "terminal", "--runtime-dir", str(RUNTIME),
         "--json", "list", "--workspace", WORKSPACE_ID],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
        timeout=15,
    )
    if completed.returncode:
        raise RuntimeError("private Agent terminal inventory failed")
    data = json.loads(completed.stdout)
    return {
        item["handle"]
        for item in data.get("items", [])
        if isinstance(item, dict) and item.get("workspaceId") == WORKSPACE_ID
        and item.get("running") is True and isinstance(item.get("handle"), str)
    }


def private_processes() -> list[dict]:
    # One local WMI request is sufficient for both providers. Discard every
    # unrelated process before the Python process sees its full command line.
    # A profiler or test PowerShell process cannot count as an Agent.
    script = """
    @(Get-CimInstance Win32_Process | Where-Object {
      $_.Name -in @('codex.exe','node.exe','claude.exe','alera.exe') -and
      $_.CommandLine -match 'e2e_host_restart' -and
      $_.CommandLine -match 'alera-workbench|mcp.+serve'
    } | ForEach-Object {
      [PSCustomObject]@{
        name=$_.Name; pid=$_.ProcessId; parent=$_.ParentProcessId;
        codex=($_.CommandLine -match 'mcp_servers.alera-workbench');
        claude=($_.CommandLine -match '--mcp-config');
        mcp=($_.Name -eq 'alera.exe' -and $_.CommandLine -match 'mcp.+serve');
      }
    }) | ConvertTo-Json -Compress
    """
    check = subprocess.run(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
        timeout=35,
    )
    if check.returncode:
        raise RuntimeError("private Windows process inspection failed")
    parsed = json.loads(check.stdout) if check.stdout.strip() else []
    if isinstance(parsed, dict):
        parsed = [parsed]
    return parsed


def verify(codex_handle: str, claude_handle: str) -> None:
    if "feat-alera-mcp-control-20261102" not in str(BUILD):
        raise RuntimeError("refusing an unpinned MCP test build")
    if RUNTIME.resolve() != (BUILD / "e2e_host_restart/runtime").resolve():
        raise RuntimeError("refusing a non-disposable Runtime Host")
    cli = BUILD / "runner" / "Debug" / "resources" / "alera" / "alera.exe"
    if not cli.is_file():
        raise RuntimeError("private MCP CLI is unavailable")
    handles = live_sessions(cli)
    processes = private_processes()
    codex = any(item.get("codex") is True for item in processes)
    claude = any(item.get("claude") is True for item in processes)
    server = any(item.get("mcp") is True for item in processes)
    checks = {
        "codexHostManagedPty": codex_handle in handles,
        "claudeHostManagedPty": claude_handle in handles,
        "codexActualProcessHasTransientMcpConfig": codex,
        "claudeActualProcessHasTransientMcpConfig": claude,
        "providerSpawnedPrivateMcpServer": server,
    }
    report = {
        "checks": checks,
        "passed": sum(checks.values()),
        "total": len(checks),
        "scope": "real Host Managed Agent launch + OS process injection",
        "doesNotProve": [
            "Claude Workspace trust approval",
            "model-initiated tool call inside Managed Agent",
            "model-replied task completion",
            "cross-workspace authorization",
        ],
        # The harness itself only reads Host/OS state and never writes a
        # Codex/Claude MCP config. Provider-internal session/cache writes
        # are outside the scope of this assertion.
        "harnessWroteGlobalMcpConfig": False,
    }
    dest = BUILD / "e2e_host_restart/mcp_managed_profile_injection_result.json"
    dest.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"MANAGED_PROFILE_INJECTION_{report['passed']}/{report['total']}", flush=True)
    for name, passed in checks.items():
        print(f"{'PASS' if passed else 'FAIL'} {name}", flush=True)
    if not all(checks.values()):
        raise AssertionError("a Managed Agent MCP injection acceptance gate did not pass")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--codex-handle", required=True)
    parser.add_argument("--claude-handle", required=True)
    args = parser.parse_args()
    verify(args.codex_handle, args.claude_handle)
