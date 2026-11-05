"""Check installed Codex/Claude MCP flags without invoking a model.

Uses private CODEX_HOME/CLAUDE_CONFIG_DIR below ignored build. No global MCP
configuration, credentials, or real model conversations are accessed.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build" / "windows" / "shared_gui_ninja"
REPORT = BUILD / "mcp_installed_cli_compat_result.json"
SERVER = "alera-workbench"


def run(cmd: list[str], env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd, env=env, capture_output=True, text=True,
        encoding="utf-8", errors="replace", timeout=21,
    )


def main(exe: Path, runtime: Path) -> None:
    if os.name != "nt" or "feat-alera-mcp-control-20261102" not in str(ROOT):
        raise RuntimeError("this probe requires the pinned Windows MCP worktree")
    if not str(exe).startswith(str((BUILD / "runner" / "Debug").resolve())):
        raise RuntimeError("only the private Debug Alera MCP executable is allowed")
    if not exe.is_file() or not runtime.is_dir():
        raise RuntimeError("private CLI or runtime path is missing")
    codex = shutil.which("codex.cmd")
    claude = shutil.which("claude.exe")
    if not codex or not claude:
        raise RuntimeError("both installed native Agent CLIs are required")
    args = ["mcp", "--runtime-dir", str(runtime), "serve"]
    private = BUILD / "mcp_provider_cli_probe"
    env = os.environ.copy()
    env["CODEX_HOME"] = str(private / "isolated-codex-home")
    env["CLAUDE_CONFIG_DIR"] = str(private / "isolated-claude-home")
    for key in [
        "ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN",
        "CLAUDE_AUTH_TOKEN", "CLAUDE_SESSION_KEY", "OPENAI_API_KEY",
    ]:
        env.pop(key, None)
    Path(env["CODEX_HOME"]).mkdir(parents=True, exist_ok=True)
    Path(env["CLAUDE_CONFIG_DIR"]).mkdir(parents=True, exist_ok=True)
    codex_flags = [
        "--config", f"mcp_servers.{SERVER}.command={json.dumps(str(exe))}",
        "--config", f"mcp_servers.{SERVER}.args={json.dumps(args)}",
    ]
    codex_list = run([codex, *codex_flags, "mcp", "list", "--json"], env)
    if codex_list.returncode:
        raise AssertionError("native Codex CLI rejected per-launch MCP configuration")
    items = json.loads(codex_list.stdout)
    matched = [item for item in items if item.get("name") == SERVER]
    if len(matched) != 1:
        raise AssertionError("Codex did not discover one Alera MCP registration")
    transport = matched[0].get("transport", {})
    if (transport.get("type") != "stdio" or
            transport.get("command") != str(exe) or
            transport.get("args") != args):
        raise AssertionError("Codex MCP command/args do not match the private Host")
    print("INSTALLED_CLI_PASS codex_native_mcp_registration", flush=True)

    claude_config = json.dumps({
        "mcpServers": {SERVER: {"command": str(exe), "args": args}},
    }, separators=(",", ":"))
    # Claude's --mcp-config consumes multiple arguments, so actual managed
    # prompts must follow the -- option terminator. --version proves only
    # native flag recognition; not MCP startup or model/tool execution.
    claude_version = run([claude, "--mcp-config", claude_config, "--version"], env)
    if claude_version.returncode or "Claude Code" not in claude_version.stdout:
        raise AssertionError("native Claude CLI rejected the per-launch flag")
    print("INSTALLED_CLI_PASS claude_native_mcp_flag", flush=True)

    sample = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "native-compat", "version": "1"},
        }},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
    ]
    child = subprocess.run(
        [str(exe), *args],
        input="\n".join(json.dumps(item, separators=(",", ":")) for item in sample) + "\n",
        capture_output=True, text=True, encoding="utf-8", errors="replace",
        timeout=18, env=env,
    )
    frames = [json.loads(line) for line in child.stdout.splitlines() if line.strip()]
    if child.returncode or len(frames) != 2:
        raise AssertionError("Alera bundled MCP stdio did not reply to initialize and tools/list")
    tools = frames[1].get("result", {}).get("tools", [])
    if len(tools) != 18 or not any(tool.get("name") == "alera.update_setting" for tool in tools):
        raise AssertionError("bundled MCP tool inventory differs from 18-tool contract")
    print("INSTALLED_CLI_PASS bundled_mcp_stdio_18_tools", flush=True)

    REPORT.write_text(json.dumps({
        "passed": 3,
        "cases": [
            "codex_native_mcp_registration",
            "claude_native_mcp_flag",
            "bundled_mcp_stdio_18_tools",
        ],
        "codexVersion": run([codex, "--version"], env).stdout.strip(),
        "claudeVersion": claude_version.stdout.strip(),
        "scope": "native CLI flag parsing and MCP stdio; NOT real model response",
        "globalConfigurationModified": False,
    }, ensure_ascii=False, indent=2), encoding="utf-8")
    print("INSTALLED_CLI_SUMMARY 3 passed", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    opts = parser.parse_args()
    main(opts.cli.resolve(), opts.runtime.resolve())
