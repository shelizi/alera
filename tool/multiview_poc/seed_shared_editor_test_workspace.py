"""Seed a disposable Alera Runtime project + real editor tab in the E2E profile.

Only run after the isolated test Alera executable has initialized its schema
and exited. Never touches the default Alera profile or a real project.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
from pathlib import Path
import json
import sqlite3
import sys

ROOT = Path(__file__).resolve().parents[2]
PROJECT_ID = "alera-e2e-shared-window-project"
WORKSPACE_ID = "alera-e2e-shared-window-workspace"
TAB_ID = "alera-e2e-shared-window-editor"
TEST_FILE = "handoff_test.dart"
ORIGINAL_TEXT = """void main() {
  // ALERA_E2E_ORIGINAL_DO_NOT_EDIT_OUTSIDE_TEST
  final message = 'Editor handoff original text';
  print(message);
}
"""


def tab_details(tab_kind: str) -> tuple[str, dict[str, object]]:
    if tab_kind == "editor":
        return TEST_FILE, {"filePath": TEST_FILE, "preview": False}
    if tab_kind == "terminal":
        return "E2E Terminal", {"terminalSessionId": "alera-e2e-shared-terminal-session"}
    raise ValueError("Unsupported E2E tab kind")


def seed_local_workbench(
    db: sqlite3.Connection, *, project_path: Path, seconds: int, tab_kind: str
) -> None:
    """Seed the disposable local Drift cache, not just its Runtime Host peer.

    Dart Drift's DateTime columns use Unix seconds, unlike the Runtime Host
    JSON/ISO-8601 timestamps. Without local rows, a cold offline launcher may
    display Welcome despite a perfectly populated runtime.sqlite.
    """
    needed = {"projects_table", "workspaces_table", "workspace_tabs_table"}
    present = {
        row[0]
        for row in db.execute("SELECT name FROM sqlite_master WHERE type='table'")
    }
    if not needed.issubset(present):
        raise RuntimeError("Unexpected local Drift schema; missing " + repr(needed - present))
    title, payload = tab_details(tab_kind)
    db.execute("BEGIN IMMEDIATE")
    try:
        db.execute(
            """INSERT INTO projects_table(id,name,repo_path,created_at,updated_at,kind)
               VALUES (?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET name=excluded.name,
                 repo_path=excluded.repo_path, updated_at=excluded.updated_at,
                 kind=excluded.kind""",
            (PROJECT_ID, "Alera Shared Editor E2E", str(project_path),
             seconds, seconds, "folder"),
        )
        db.execute(
            """INSERT INTO workspaces_table(id,project_id,name,branch,path,
                 created_at,updated_at,kind,status,source_branch,
                 reuses_existing_branch,is_pinned,archived_at)
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET name=excluded.name,
                 path=excluded.path, updated_at=excluded.updated_at,
                 status=excluded.status, is_pinned=excluded.is_pinned,
                 archived_at=NULL""",
            (WORKSPACE_ID, PROJECT_ID, "E2E Workspace", None,
             str(project_path), seconds, seconds, "main", "active", None,
             0, 1, None),
        )
        db.execute(
            """INSERT INTO workspace_tabs_table(id,workspace_id,kind,title,
                 created_at,updated_at,payload_json)
               VALUES (?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET workspace_id=excluded.workspace_id,
                 kind=excluded.kind,title=excluded.title,
                 updated_at=excluded.updated_at,
                 payload_json=excluded.payload_json""",
            (TAB_ID, WORKSPACE_ID, tab_kind, title, seconds, seconds,
             json.dumps(payload)),
        )
        db.commit()
    except Exception:
        db.rollback()
        raise


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reset-test-file", action="store_true")
    parser.add_argument(
        "--tab-kind",
        choices=("editor", "terminal"),
        default="editor",
        help="Use a disposable Terminal tab instead of the default Editor tab",
    )
    parser.add_argument(
        "--configuration",
        choices=("Debug", "Release"),
        default="Debug",
        help="Select an isolated build profile; never use Alera's real settings",
    )
    args = parser.parse_args()
    build = ROOT / "build" / "windows" / (
        "shared_release_ninja" if args.configuration == "Release"
        else "shared_gui_ninja"
    )
    profile = build / "e2e_support"
    runtime_db = profile / "terminal_host" / "runtime.sqlite"
    local_db = profile / "alera.sqlite"
    project_dir = build / "e2e_workspace"
    project_path = project_dir.resolve()
    runtime_path = runtime_db.resolve()
    if not project_path.is_relative_to(build.resolve()):
        raise RuntimeError("Refusing project outside ignored worktree build")
    if not runtime_path.is_relative_to(build.resolve()):
        raise RuntimeError("Refusing database outside ignored worktree build")
    if not runtime_path.is_file():
        raise RuntimeError("Start/stop isolated E2E Alera once to initialize Runtime DB first")
    local_path = local_db.resolve()
    if not local_path.is_relative_to(build.resolve()):
        raise RuntimeError("Refusing local database outside ignored worktree build")
    if not local_path.is_file():
        raise RuntimeError("Start/stop isolated E2E Alera once to initialize local Drift DB")
    project_dir.mkdir(parents=True, exist_ok=True)
    file = project_dir / TEST_FILE
    if args.reset_test_file or not file.exists():
        file.write_text(ORIGINAL_TEXT, encoding="utf-8")
    now = datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
    seconds = int(datetime.now(timezone.utc).timestamp())
    db = sqlite3.connect(runtime_path)
    try:
        needed = {"projects", "workspaces", "workspaceTabs"}
        present = {row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}
        if not needed.issubset(present):
            raise RuntimeError("Unexpected Runtime schema; need "+repr(needed-present))
        db.execute("BEGIN IMMEDIATE")
        db.execute(
            """INSERT INTO projects(id,name,repoPath,createdAt,updatedAt,kind)
               VALUES (?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET name=excluded.name, repoPath=excluded.repoPath,
               updatedAt=excluded.updatedAt, kind=excluded.kind""",
            (PROJECT_ID, "Alera Shared Editor E2E", str(project_path), now, now, "folder"),
        )
        db.execute(
            """INSERT INTO workspaces(id,instanceId,hostId,projectId,name,branch,path,
                  createdAt,updatedAt,kind,status,sourceBranch,reusesExistingBranch,isPinned,archivedAt)
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET path=excluded.path,updatedAt=excluded.updatedAt,
                 status=excluded.status,archivedAt=NULL""",
            (WORKSPACE_ID, WORKSPACE_ID, "local", PROJECT_ID, "E2E Workspace",
             None, str(project_path), now, now, "main", "active", None, 0, 1, None),
        )
        tab_title, tab_payload = tab_details(args.tab_kind)
        db.execute(
            """INSERT INTO workspaceTabs(id,workspaceId,kind,title,createdAt,updatedAt,payloadJson)
               VALUES (?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET workspaceId=excluded.workspaceId, kind=excluded.kind,
                 title=excluded.title,payloadJson=excluded.payloadJson""",
            (TAB_ID, WORKSPACE_ID, args.tab_kind, tab_title, now, now,
             json.dumps(tab_payload)),
        )
        db.commit()
        print("SEED_OK", "test_db="+str(runtime_path), "project="+str(project_path),
              "tab="+TAB_ID, "kind="+args.tab_kind,
              "file_sha_seed_bytes="+str(file.stat().st_size))
    except Exception:
        db.rollback()
        raise
    finally:
        db.close()
    local = sqlite3.connect(local_path)
    try:
        seed_local_workbench(
            local, project_path=project_path, seconds=seconds,
            tab_kind=args.tab_kind,
        )
    finally:
        local.close()
    print("SEED_LOCAL_OK", "local_db="+str(local_path),
          "project="+PROJECT_ID, "workspace="+WORKSPACE_ID,
          "tab="+TAB_ID, "kind="+args.tab_kind)


if __name__ == "__main__":
    main()
