"""Add a second editor tab to the disposable E2E workspace for multi-tab
detached-window validation. Run seed_shared_editor_test_workspace.py first;
both Alera processes must be stopped before writing either database.
"""
from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path
import json
import sqlite3
import sys

ROOT = Path(__file__).resolve().parents[2]
WORKSPACE_ID = "alera-e2e-shared-window-workspace"
TAB2_ID = "alera-e2e-shared-window-editor-2"
TEST_FILE2 = "handoff_test_second.dart"
TEXT2 = """void second() {
  // ALERA_E2E_SECOND_TAB_DO_NOT_EDIT_OUTSIDE_TEST
  print('second tab file');
}
"""


def main() -> None:
    build = ROOT / "build" / "windows" / "shared_gui_ninja"
    profile = build / "e2e_support"
    runtime_db = (profile / "terminal_host" / "runtime.sqlite").resolve()
    local_db = (profile / "alera.sqlite").resolve()
    project_dir = build / "e2e_workspace"
    file2 = project_dir / TEST_FILE2
    for path in (runtime_db, local_db):
        if not str(path).startswith(str(build.resolve())):
            raise RuntimeError("Refusing database outside ignored worktree build")
        if not path.is_file():
            raise RuntimeError("Seed the workspace first; missing " + str(path))
    file2.write_text(TEXT2, encoding="utf-8")
    now = datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
    seconds = int(datetime.now(timezone.utc).timestamp())
    payload = json.dumps({"filePath": TEST_FILE2, "preview": False})

    runtime = sqlite3.connect(runtime_db)
    try:
        runtime.execute(
            """INSERT INTO workspaceTabs(id,workspaceId,kind,title,createdAt,updatedAt,payloadJson)
               VALUES (?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET workspaceId=excluded.workspaceId, kind=excluded.kind,
                 title=excluded.title,payloadJson=excluded.payloadJson""",
            (TAB2_ID, WORKSPACE_ID, "editor", TEST_FILE2, now, now, payload),
        )
        runtime.commit()
    finally:
        runtime.close()

    local = sqlite3.connect(local_db)
    try:
        local.execute(
            """INSERT INTO workspace_tabs_table(id,workspace_id,kind,title,
                 created_at,updated_at,payload_json)
               VALUES (?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET workspace_id=excluded.workspace_id,
                 kind=excluded.kind,title=excluded.title,
                 updated_at=excluded.updated_at,payload_json=excluded.payload_json""",
            (TAB2_ID, WORKSPACE_ID, "editor", TEST_FILE2, seconds, seconds, payload),
        )
        local.commit()
    finally:
        local.close()
    print("SEED_TAB2_OK", "tab=" + TAB2_ID, "file=" + str(file2))


if __name__ == "__main__":
    main()
