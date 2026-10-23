"""Regression tests for the isolated shared-window E2E fixture.

These use in-memory SQLite databases and never open Alera's real profile.
"""

from pathlib import Path
import json
import sqlite3
import unittest

import seed_shared_editor_test_workspace as fixture


def _local_database() -> sqlite3.Connection:
    db = sqlite3.connect(":memory:")
    db.executescript(
        """
        CREATE TABLE projects_table (
          id TEXT PRIMARY KEY, name TEXT NOT NULL, repo_path TEXT NOT NULL,
          created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
          kind TEXT NOT NULL
        );
        CREATE TABLE workspaces_table (
          id TEXT PRIMARY KEY, project_id TEXT NOT NULL, name TEXT NOT NULL,
          branch TEXT, path TEXT NOT NULL, created_at INTEGER NOT NULL,
          updated_at INTEGER NOT NULL, kind TEXT NOT NULL, status TEXT NOT NULL,
          source_branch TEXT, reuses_existing_branch INTEGER NOT NULL,
          is_pinned INTEGER NOT NULL, archived_at INTEGER
        );
        CREATE TABLE workspace_tabs_table (
          id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL, kind TEXT NOT NULL,
          title TEXT NOT NULL, created_at INTEGER NOT NULL,
          updated_at INTEGER NOT NULL, payload_json TEXT NOT NULL
        );
        """
    )
    return db


class E2ESeedTests(unittest.TestCase):
    def test_local_database_contains_same_project_workspace_and_terminal(self) -> None:
        with _local_database() as db:
            fixture.seed_local_workbench(
                db,
                project_path=Path('D:/isolated/build/e2e_workspace'),
                seconds=1792751347,
                tab_kind='terminal',
            )
            project = db.execute('SELECT id, kind FROM projects_table').fetchone()
            workspace = db.execute(
                'SELECT id, project_id, status, is_pinned FROM workspaces_table'
            ).fetchone()
            tab = db.execute(
                'SELECT id, workspace_id, kind, title, payload_json '
                'FROM workspace_tabs_table'
            ).fetchone()

        self.assertEqual(project, (fixture.PROJECT_ID, 'folder'))
        self.assertEqual(
            workspace,
            (fixture.WORKSPACE_ID, fixture.PROJECT_ID, 'active', 1),
        )
        self.assertEqual(tab[:4], (
            fixture.TAB_ID, fixture.WORKSPACE_ID, 'terminal', 'E2E Terminal',
        ))
        self.assertEqual(
            json.loads(tab[4]),
            {'terminalSessionId': 'alera-e2e-shared-terminal-session'},
        )

    def test_local_seed_is_idempotent_and_can_switch_tab_kind(self) -> None:
        with _local_database() as db:
            params = dict(
                project_path=Path('D:/isolated/build/e2e_workspace'),
                seconds=1792751347,
            )
            fixture.seed_local_workbench(db, tab_kind='terminal', **params)
            fixture.seed_local_workbench(db, tab_kind='editor', **params)
            for table in ('projects_table', 'workspaces_table', 'workspace_tabs_table'):
                self.assertEqual(db.execute('SELECT count(*) FROM ' + table).fetchone()[0], 1)
            tab = db.execute('SELECT kind, payload_json FROM workspace_tabs_table').fetchone()
            self.assertEqual(tab[0], 'editor')
            self.assertEqual(json.loads(tab[1]), {'filePath': fixture.TEST_FILE, 'preview': False})

    def test_missing_local_schema_is_rejected(self) -> None:
        with sqlite3.connect(':memory:') as db:
            with self.assertRaisesRegex(RuntimeError, 'local.*schema'):
                fixture.seed_local_workbench(
                    db,
                    project_path=Path('D:/isolated/build/e2e_workspace'),
                    seconds=1792751347,
                    tab_kind='terminal',
                )


if __name__ == '__main__':
    unittest.main()
