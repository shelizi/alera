import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_listing.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_section.dart';
import 'package:flutter_test/flutter_test.dart';

final DateTime _now = .utc(2026, 9, 1);
final DateTime _archivedAt = .utc(2026, 8, 1);

Project _project(String id) {
  return Project(
    id: id,
    name: id,
    repoPath: '/repo/$id',
    createdAt: _now,
    updatedAt: _now,
  );
}

Workspace _workspace(
  String id,
  String projectId, {
  WorkspaceKind kind = .linked,
  bool pinned = false,
  bool archived = false,
  String? sectionId,
}) {
  return Workspace(
    id: id,
    projectId: projectId,
    name: id,
    path: '/repo/$projectId/$id',
    createdAt: _now,
    updatedAt: _now,
    kind: kind,
    status: .active,
    isPinned: pinned,
    sectionId: sectionId,
    archivedAt: archived ? _archivedAt : null,
  );
}

WorkbenchState _state({
  WorkbenchViewPrefs prefs = WorkbenchViewPrefs.defaults,
}) {
  return WorkbenchState(
    projects: <Project>[_project('alera'), _project('orca')],
    workspacesByProject: <String, List<Workspace>>{
      'alera': <Workspace>[
        _workspace('alera-main', 'alera', kind: .main),
        _workspace('feature', 'alera'),
        _workspace('old-side', 'alera', archived: true),
        _workspace('pinned-old', 'alera', pinned: true, archived: true),
      ],
      'orca': <Workspace>[
        _workspace('orca-main', 'orca', kind: .main),
        _workspace('stale', 'orca', archived: true),
      ],
    },
    viewPrefs: prefs,
  );
}

List<String> _rowKeys(List<WorkbenchSidebarRow> rows) {
  return rows.map((row) => row.key).toList();
}

void main() {
  test('project grouping appends a collapsible Archived block per project', () {
    final rows = buildSidebarRows(_state());

    expect(_rowKeys(rows), <String>[
      'project:alera',
      'workspace:all:alera-main',
      'workspace:all:feature',
      'archived:alera',
      'workspace:all:old-side',
      'workspace:all:pinned-old',
      'project:orca',
      'workspace:all:orca-main',
      'archived:orca',
      'workspace:all:stale',
    ]);

    final headers = rows.whereType<WorkbenchArchivedHeaderRow>().toList();
    expect(headers.map((row) => row.workspaceCount), <int>[2, 1]);
    expect(headers.every((row) => row.indent == 1), isTrue);
    final archivedRows = rows
        .whereType<WorkbenchWorkspaceRow>()
        .where((row) => row.workspace.isArchived)
        .toList();
    expect(archivedRows.every((row) => row.indent == 2), isTrue);
    expect(archivedRows.every((row) => !row.showProjectChip), isTrue);

    // Archived rows keep project header counts accurate and stay out of the
    // pinned section entirely.
    expect(
      rows.whereType<WorkbenchProjectHeaderRow>().map(
        (row) => row.workspaceCount,
      ),
      <int>[2, 1],
    );
    expect(rows.whereType<WorkbenchPinnedHeaderRow>(), isEmpty);
    expect(
      rows.whereType<WorkbenchWorkspaceRow>().any((row) => row.isPinnedCopy),
      isFalse,
    );
  });

  test('section grouping emits one global Archived group after sections', () {
    final state =
        _state(
          prefs: WorkbenchViewPrefs.defaults.copyWith(
            groupBy: WorkbenchGroupBy.section,
          ),
        ).copyWith(
          sections: <WorkspaceSection>[
            WorkspaceSection(
              id: 'alpha',
              name: 'Alpha',
              createdAt: _now,
              updatedAt: _now,
            ),
          ],
          workspacesByProject: <String, List<Workspace>>{
            'alera': <Workspace>[
              _workspace(
                'alera-main',
                'alera',
                kind: .main,
                sectionId: 'alpha',
              ),
              _workspace('feature', 'alera'),
              _workspace(
                'old-side',
                'alera',
                archived: true,
                sectionId: 'alpha',
              ),
              _workspace('pinned-old', 'alera', pinned: true, archived: true),
            ],
            'orca': <Workspace>[
              _workspace('orca-main', 'orca', kind: .main),
              _workspace('stale', 'orca', archived: true),
            ],
          },
        );

    final rows = buildSidebarRows(state);
    final header = rows.whereType<WorkbenchArchivedHeaderRow>().single;
    expect(header.key, 'archived:global');
    expect(header.workspaceCount, 3);
    expect(header.indent, 0);

    final headerIndex = rows.indexOf(header);
    // The Archived group sits behind every section header.
    for (final row in rows.whereType<WorkbenchSectionHeaderRow>()) {
      expect(rows.indexOf(row), lessThan(headerIndex));
    }
    final archivedRows = rows
        .sublist(headerIndex + 1)
        .whereType<WorkbenchWorkspaceRow>()
        .toList();
    expect(archivedRows, hasLength(3));
    expect(archivedRows.every((row) => row.workspace.isArchived), isTrue);
    expect(archivedRows.every((row) => row.indent == 1), isTrue);
    expect(archivedRows.every((row) => row.showProjectChip), isTrue);
  });

  test(
    'flat grouping appends one Archived header after the workspace list',
    () {
      final prefs = WorkbenchViewPrefs.defaults.copyWith(
        groupBy: WorkbenchGroupBy.none,
      );
      final rows = buildSidebarRows(_state(prefs: prefs));

      final header = rows.whereType<WorkbenchArchivedHeaderRow>().single;
      expect(header.key, 'archived:global');
      expect(header.workspaceCount, 3);
      expect(header.indent, 0);
      expect(rows.indexOf(header), rows.length - 4);
      final archivedRows = rows
          .sublist(rows.indexOf(header) + 1)
          .whereType<WorkbenchWorkspaceRow>()
          .toList();
      expect(archivedRows.every((row) => row.indent == 0), isTrue);
      expect(archivedRows.every((row) => row.showProjectChip), isTrue);
      expect(rows.whereType<WorkbenchPinnedHeaderRow>(), isEmpty);
    },
  );

  test('archived rows stay out of counts and collapse targets', () {
    final state = _state();

    expect(countVisibleWorkspaces(state), 3);
    final targets = visibleSidebarCollapseTargets(state);
    expect(targets.workspaceIds, <String>{
      'alera-main',
      'feature',
      'orca-main',
    });
  });

  test('search still surfaces archived workspaces under the header', () {
    final state = _state().copyWith(searchQuery: 'stale');
    final rows = buildSidebarRows(state);

    expect(_rowKeys(rows), <String>[
      'project:orca',
      'archived:orca',
      'workspace:all:stale',
    ]);
    // The visible count intentionally ignores archived matches.
    expect(countVisibleWorkspaces(state), 0);
  });

  test('a project with only archived workspaces still renders its header', () {
    final rows = buildSidebarRows(_state().copyWith(searchQuery: 'old-side'));

    expect(_rowKeys(rows), <String>[
      'project:alera',
      'archived:alera',
      'workspace:all:old-side',
    ]);
  });

  test('collapseArchivedSidebarRows hides only the collapsed scope', () {
    final rows = buildSidebarRows(_state());

    final collapsed = collapseArchivedSidebarRows(rows, <String>{
      'archived:alera',
    });

    expect(_rowKeys(collapsed), <String>[
      'project:alera',
      'workspace:all:alera-main',
      'workspace:all:feature',
      'archived:alera',
      'project:orca',
      'workspace:all:orca-main',
      'archived:orca',
      'workspace:all:stale',
    ]);

    // Collapsing the global key (section/none grouping idiom) hides its rows.
    final flatRows = buildSidebarRows(
      _state(
        prefs: WorkbenchViewPrefs.defaults.copyWith(
          groupBy: WorkbenchGroupBy.none,
        ),
      ),
    );
    final collapsedFlat = collapseArchivedSidebarRows(flatRows, <String>{
      'archived:global',
    });
    expect(collapsedFlat.whereType<WorkbenchWorkspaceRow>(), hasLength(3));
    expect(
      collapsedFlat.whereType<WorkbenchWorkspaceRow>().every(
        (row) => !row.workspace.isArchived,
      ),
      isTrue,
    );

    // An empty collapse set returns the rows untouched.
    expect(
      identical(collapseArchivedSidebarRows(rows, const <String>{}), rows),
      isTrue,
    );
  });
}
