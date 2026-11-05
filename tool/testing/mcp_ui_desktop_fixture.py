"""Prepare and spawn the disposable Windows Alera MCP desktop E2E fixture.

Never targets a user profile or the normal Alera runtime. The fixture project,
Git repository and Agent PTY are under this worktree's ignored build directory.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess

from mcp_ui_agent_desktop_e2e import HostRpc

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build" / "windows" / "shared_gui_ninja"
WORKSPACE = BUILD / "e2e_workspace"
RUNTIME = BUILD / "e2e_support" / "terminal_host"
WORKSPACE_ID = "alera-e2e-shared-window-workspace"
SESSION_ID = "alera-mcp-e2e-agent-session"
TAB_ID = "alera-mcp-e2e-agent-tab"


def run_git(*args: str, path: Path) -> None:
    subprocess.run(
        ["git", "-C", str(path), *args],
        check=True, capture_output=True, text=True, timeout=25,
    )


def prepare() -> None:
    # This never creates/updates the production project or Git repository.
    if not (BUILD / "e2e_support" / "alera.sqlite").exists():
        raise RuntimeError("Isolated GUI must run and exit once to seed its Drift schema")
    if not (RUNTIME / "runtime.sqlite").exists():
        raise RuntimeError("Isolated GUI must run and exit once to seed its Host schema")
    seed = ROOT / "tool" / "multiview_poc" / "seed_shared_editor_test_workspace.py"
    subprocess.run(
        ["python", str(seed), "--configuration", "Debug", "--tab-kind", "editor"],
        cwd=str(ROOT), check=True, timeout=35,
    )
    needle = WORKSPACE / "src" / "deep" / "needle.dart"
    needle.parent.mkdir(parents=True, exist_ok=True)
    needle.write_text("void main() { print('MCP_E2E_NEEDLE'); }\n", encoding="utf-8")

    git_root = WORKSPACE / "git_fixture"
    git_root.mkdir(exist_ok=True)
    if not (git_root / ".git").exists():
        subprocess.run(["git", "init", "-b", "main", str(git_root)],
                       check=True, capture_output=True, timeout=25)
        (git_root / "README.md").write_text("# MCP Git E2E\n", encoding="utf-8")
        run_git("add", "README.md", path=git_root)
        run_git("-c", "user.name=MCP Test", "-c", "user.email=mcp-test@example.invalid",
                "commit", "-m", "MCP fixture base", path=git_root)
        run_git("checkout", "-b", "feature/demo", path=git_root)
        (git_root / "README.md").write_text("# MCP Git E2E\nfeature demo\n", encoding="utf-8")
        run_git("add", "README.md", path=git_root)
        run_git("-c", "user.name=MCP Test", "-c", "user.email=mcp-test@example.invalid",
                "commit", "-m", "MCP fixture change", path=git_root)
        run_git("checkout", "main", path=git_root)
    print("MCP_DESKTOP_FIXTURE_READY workspace=" + str(WORKSPACE))


def spawn_agent() -> None:
    if not WORKSPACE.is_dir() or not (WORKSPACE / "src/deep/needle.dart").is_file():
        raise RuntimeError("E2E workspace fixture not prepared")
    now = datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
    tab = {
        "id": TAB_ID,
        "workspaceId": WORKSPACE_ID,
        "kind": "terminal",
        "title": "MCP E2E Agent Shell",
        "createdAt": now,
        "updatedAt": now,
        "payload": {
            "terminalSessionId": SESSION_ID,
            "agentType": "codex",
            "spawnOnCreate": True,
        },
    }
    host = HostRpc(RUNTIME)
    try:
        result = host.request("tab.upsert", tab)
        if result.get("id") != TAB_ID:
            raise RuntimeError("Host did not persist expected E2E Agent tab")
    finally:
        host.close()
    print("MCP_E2E_AGENT_SPAWNED session=" + SESSION_ID)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("prepare", "spawn-agent"))
    opts = parser.parse_args()
    if "feat-alera-mcp-control-20261102" not in str(ROOT):
        raise RuntimeError("Refusing to operate outside the pinned isolated MCP worktree")
    if opts.mode == "prepare":
        prepare()
    else:
        spawn_agent()


if __name__ == "__main__":
    main()
