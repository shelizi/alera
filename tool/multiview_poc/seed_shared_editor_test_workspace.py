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


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reset-test-file", action="store_true")
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
    project_dir = build / "e2e_workspace"
    project_path = project_dir.resolve()
    runtime_path = runtime_db.resolve()
    if not project_path.is_relative_to(build.resolve()):
        raise RuntimeError("Refusing project outside ignored worktree build")
    if not runtime_path.is_relative_to(build.resolve()):
        raise RuntimeError("Refusing database outside ignored worktree build")
    if not runtime_path.is_file():
        raise RuntimeError("Start/stop isolated E2E Alera once to initialize Runtime DB first")
    project_dir.mkdir(parents=True, exist_ok=True)
    file = project_dir / TEST_FILE
    if args.reset_test_file or not file.exists():
        file.write_text(ORIGINAL_TEXT, encoding="utf-8")
    now = datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
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
        db.execute(
            """INSERT INTO workspaceTabs(id,workspaceId,kind,title,createdAt,updatedAt,payloadJson)
               VALUES (?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET workspaceId=excluded.workspaceId, kind=excluded.kind,
                 title=excluded.title,payloadJson=excluded.payloadJson""",
            (TAB_ID, WORKSPACE_ID, "editor", TEST_FILE, now, now,
             json.dumps({"filePath":TEST_FILE,"preview":False})),
        )
        db.commit()
        print("SEED_OK", "test_db="+str(runtime_path), "project="+str(project_path),
              "tab="+TAB_ID, "file_sha_seed_bytes="+str(file.stat().st_size))
    except Exception:
        db.rollback()
        raise
    finally:
        db.close()


if __name__ == "__main__":
    main()
