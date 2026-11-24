import 'dart:collection';

import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_listing.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

final DateTime _now = .utc(2026, 7, 16);

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
  required String name,
  bool isPinned = false,
  String? parentWorkspaceId,
}) {
  return Workspace(
    id: id,
    projectId: projectId,
    name: name,
    branch: 'feature/$id',
    path: '/repo/$projectId/$id',
    createdAt: _now,
    updatedAt: _now,
    kind: .linked,
    status: .active,
    isPinned: isPinned,
    parentWorkspaceId: parentWorkspaceId,
  );
}

WorkbenchState _state({
  WorkbenchViewPrefs prefs = WorkbenchViewPrefs.defaults,
  String searchQuery = '',
  bool pinFeature = true,
}) {
  final alera = _project('alera');
  final orca = _project('orca');
  return WorkbenchState(
    projects: <Project>[alera, orca],
    workspacesByProject: <String, List<Workspace>>{
      alera.id: <Workspace>[
        _workspace('main', alera.id, name: 'Main'),
        _workspace('feature', alera.id, name: 'Feature', isPinned: pinFeature),
      ],
      orca.id: <Workspace>[_workspace('review', orca.id, name: 'Review')],
    },
    viewPrefs: prefs,
    searchQuery: searchQuery,
  );
}

void main() {
  test('indexes shared manual workspace order once per sidebar projection', () {
    final order = _CountingOrder([
      ...List<String>.generate(1000, (i) => 'hidden-$i'),
      'main',
      'feature',
      'review',
    ]);
    final prefs = WorkbenchViewPrefs(
      groupBy: WorkbenchGroupBy.project,
      projectSort: WorkbenchSortBy.name,
      workspaceSort: WorkbenchSortBy.manual,
      workspaceOrder: order,
      selectedProjectIds: {},
      collapsedProjectIds: {},
      expandedWorkspaceIds: {},
    );
    final rows = buildSidebarRows(_state(prefs: prefs));
    expect(workspaceOrderOfRows(rows), ['main', 'feature', 'review']);
    expect(
      rows.whereType<WorkbenchWorkspaceRow>().where((r) => r.isPinnedCopy),
      hasLength(1),
    );
    expect(order.reads, order.length);
  });

  test('collapsed pins do not project hidden agent runs in any grouping', () {
    final project = _project('p');
    final workspace = _workspace(
      'w',
      project.id,
      name: 'Pinned',
      isPinned: true,
    );
    final tab = WorkspaceTabRecord(
      id: 't',
      workspaceId: workspace.id,
      title: 'Agent',
      kind: WorkspaceTabKind.terminal,
      createdAt: _now,
      updatedAt: _now,
    );
    for (final grouping in WorkbenchGroupBy.values) {
      final statuses = _CountingStatuses();
      final prefs = WorkbenchViewPrefs.defaults.copyWith(
        groupBy: grouping,
        pinnedSectionCollapsed: true,
        showPinnedWorkspacesBelow: false,
      );
      final state = WorkbenchState(
        projects: [project],
        workspacesByProject: {
          project.id: [workspace],
        },
        tabsByWorkspace: {
          workspace.id: [tab],
        },
        viewPrefs: prefs,
      );
      final rows = buildSidebarRows(state, agentStatuses: statuses);
      expect(
        rows.whereType<WorkbenchPinnedHeaderRow>().single.workspaceCount,
        1,
      );
      expect(rows.whereType<WorkbenchWorkspaceRow>(), isEmpty);
      expect(statuses.reads, 0);
      final expanded = buildSidebarRows(
        state.copyWith(
          viewPrefs: prefs.copyWith(pinnedSectionCollapsed: false),
        ),
        agentStatuses: statuses,
      );
      expect(
        expanded.whereType<WorkbenchWorkspaceRow>().single.isPinnedCopy,
        isTrue,
      );
      expect(statuses.reads, greaterThan(0));
    }
  });

  test('adds a global duplicate without polluting regular order memory', () {
    final state = _state();
    final rows = buildSidebarRows(state);
    final copies = rows
        .whereType<WorkbenchWorkspaceRow>()
        .where((row) => row.workspace.id == 'feature')
        .toList();

    expect((rows.first as WorkbenchPinnedHeaderRow).workspaceCount, 1);
    expect(copies, hasLength(2));
    expect(copies.where((row) => row.isPinnedCopy), hasLength(1));
    expect(
      copies.firstWhere((row) => row.isPinnedCopy).showProjectChip,
      isTrue,
    );
    expect(
      workspaceOrderOfRows(rows).where((id) => id == 'feature'),
      hasLength(1),
    );
    expect(countVisibleWorkspaces(state), 3);
  });

  test('respects filters but ignores project collapse', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      collapsedProjectIds: <String>{'alera'},
      selectedProjectIds: <String>{'alera'},
    );
    final visibleRows = buildSidebarRows(_state(prefs: prefs));
    final filteredRows = buildSidebarRows(
      _state(prefs: prefs, searchQuery: 'does-not-match'),
    );

    expect(
      visibleRows.whereType<WorkbenchWorkspaceRow>().where(
        (row) => row.isPinnedCopy,
      ),
      hasLength(1),
    );
    expect(filteredRows.whereType<WorkbenchPinnedHeaderRow>(), isEmpty);
  });

  test('can omit pinned workspaces from project sections below', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      showPinnedWorkspacesBelow: false,
    );
    final rows = buildSidebarRows(_state(prefs: prefs));
    final featureRows = rows
        .whereType<WorkbenchWorkspaceRow>()
        .where((row) => row.workspace.id == 'feature')
        .toList();

    expect(featureRows, hasLength(1));
    expect(featureRows.single.isPinnedCopy, isTrue);
    expect(
      rows
          .whereType<WorkbenchProjectHeaderRow>()
          .firstWhere((row) => row.project.id == 'alera')
          .workspaceCount,
      1,
    );
    expect(workspaceOrderOfRows(rows), <String>['main', 'review']);
    expect(countVisibleWorkspaces(_state(prefs: prefs)), 3);
  });

  test('builds a tree from pinned workspaces only', () {
    final project = _project('alera');
    final parent = _workspace(
      'parent',
      project.id,
      name: 'Parent',
      isPinned: true,
    );
    final child = _workspace(
      'child',
      project.id,
      name: 'Child',
      isPinned: true,
      parentWorkspaceId: parent.id,
    );
    final unpinnedParent = _workspace(
      'other-parent',
      project.id,
      name: 'Other Parent',
    );
    final orphanedPin = _workspace(
      'orphaned-pin',
      project.id,
      name: 'Orphaned Pin',
      isPinned: true,
      parentWorkspaceId: unpinnedParent.id,
    );
    final state = WorkbenchState(
      projects: <Project>[project],
      workspacesByProject: <String, List<Workspace>>{
        project.id: <Workspace>[parent, child, unpinnedParent, orphanedPin],
      },
    );

    final pinned = buildSidebarRows(state)
        .whereType<WorkbenchWorkspaceRow>()
        .where((row) => row.isPinnedCopy)
        .toList();

    expect(pinned.firstWhere((row) => row.workspace.id == parent.id).indent, 0);
    expect(pinned.firstWhere((row) => row.workspace.id == child.id).indent, 1);
    expect(
      pinned.firstWhere((row) => row.workspace.id == orphanedPin.id).indent,
      0,
    );
    expect(
      pinned
          .firstWhere((row) => row.workspace.id == parent.id)
          .visibleChildCount,
      1,
    );
  });

  test('collapsing a pinned parent hides its pinned children', () {
    final project = _project('alera');
    final parent = _workspace(
      'parent',
      project.id,
      name: 'Parent',
      isPinned: true,
    );
    final child = _workspace(
      'child',
      project.id,
      name: 'Child',
      isPinned: true,
      parentWorkspaceId: parent.id,
    );
    final state = WorkbenchState(
      projects: <Project>[project],
      workspacesByProject: <String, List<Workspace>>{
        project.id: <Workspace>[parent, child],
      },
      viewPrefs: WorkbenchViewPrefs.defaults.copyWith(
        collapsedParentWorkspaceIds: <String>{parent.id},
      ),
    );

    final pinned = buildSidebarRows(state)
        .whereType<WorkbenchWorkspaceRow>()
        .where((row) => row.isPinnedCopy)
        .toList();

    expect(pinned.map((row) => row.workspace.id), <String>[parent.id]);
    expect(pinned.single.childrenCollapsed, isTrue);
    expect(pinned.single.visibleChildCount, 1);
  });

  group('collapsible sections', () {
    test(
      'group by none adds a collapsible all section after the pinned copies',
      () {
        final prefs = WorkbenchViewPrefs.defaults.copyWith(groupBy: .none);
        final rows = buildSidebarRows(_state(prefs: prefs));

        final pinnedHeader = rows.first as WorkbenchPinnedHeaderRow;
        expect(pinnedHeader.workspaceCount, 1);
        expect(pinnedHeader.collapsed, isFalse);

        final allHeader = rows.whereType<WorkbenchAllHeaderRow>().single;
        expect(allHeader.workspaceCount, 3);
        expect(allHeader.collapsed, isFalse);

        final pinnedIndex = rows.indexOf(pinnedHeader);
        final allIndex = rows.indexOf(allHeader);
        final copies = rows
            .sublist(pinnedIndex + 1, allIndex)
            .whereType<WorkbenchWorkspaceRow>()
            .toList();
        expect(copies, hasLength(1));
        expect(copies.single.isPinnedCopy, isTrue);

        final regular = rows
            .sublist(allIndex + 1)
            .whereType<WorkbenchWorkspaceRow>()
            .toList();
        expect(regular, hasLength(3));
        expect(regular.every((row) => !row.isPinnedCopy), isTrue);
      },
    );

    test('flat list can exclude pinned workspaces from All', () {
      final prefs = WorkbenchViewPrefs.defaults.copyWith(
        groupBy: .none,
        showPinnedWorkspacesBelow: false,
      );
      final rows = buildSidebarRows(_state(prefs: prefs));

      expect(
        rows.whereType<WorkbenchPinnedHeaderRow>().single.workspaceCount,
        1,
      );
      expect(rows.whereType<WorkbenchAllHeaderRow>().single.workspaceCount, 2);
      final entries = rows.whereType<WorkbenchWorkspaceRow>().toList();
      expect(
        entries.where((row) => row.workspace.id == 'feature'),
        hasLength(1),
      );
      expect(workspaceOrderOfRows(rows), <String>['main', 'review']);
    });

    test(
      'flat list omits an empty All section when every workspace is pinned',
      () {
        final prefs = WorkbenchViewPrefs.defaults.copyWith(
          groupBy: .none,
          showPinnedWorkspacesBelow: false,
        );
        final project = _project('alera');
        final pinned = _workspace(
          'pinned',
          project.id,
          name: 'Pinned',
          isPinned: true,
        );
        final rows = buildSidebarRows(
          WorkbenchState(
            projects: <Project>[project],
            workspacesByProject: <String, List<Workspace>>{
              project.id: <Workspace>[pinned],
            },
            viewPrefs: prefs,
          ),
        );

        expect(rows.whereType<WorkbenchPinnedHeaderRow>(), hasLength(1));
        expect(rows.whereType<WorkbenchAllHeaderRow>(), isEmpty);
        expect(rows.whereType<WorkbenchWorkspaceRow>(), hasLength(1));
      },
    );

    test('collapsed pinned section keeps the headers and drops the copies', () {
      final prefs = WorkbenchViewPrefs.defaults.copyWith(
        groupBy: .none,
        pinnedSectionCollapsed: true,
      );
      final rows = buildSidebarRows(_state(prefs: prefs));

      expect(
        rows.whereType<WorkbenchPinnedHeaderRow>().single.collapsed,
        isTrue,
      );
      expect(rows.whereType<WorkbenchAllHeaderRow>(), hasLength(1));
      final workspaces = rows.whereType<WorkbenchWorkspaceRow>().toList();
      expect(workspaces, hasLength(3));
      expect(workspaces.every((row) => !row.isPinnedCopy), isTrue);
    });

    test(
      'collapsed all section keeps pinned copies and drops the flat list',
      () {
        final prefs = WorkbenchViewPrefs.defaults.copyWith(
          groupBy: .none,
          allSectionCollapsed: true,
        );
        final rows = buildSidebarRows(_state(prefs: prefs));

        expect(
          rows.whereType<WorkbenchAllHeaderRow>().single.collapsed,
          isTrue,
        );
        final workspaces = rows.whereType<WorkbenchWorkspaceRow>().toList();
        expect(workspaces, hasLength(1));
        expect(workspaces.single.isPinnedCopy, isTrue);
      },
    );

    test(
      'group by project has no all section but still collapses pinned copies',
      () {
        final prefs = WorkbenchViewPrefs.defaults.copyWith(
          pinnedSectionCollapsed: true,
        );
        final rows = buildSidebarRows(_state(prefs: prefs));

        expect(rows.whereType<WorkbenchAllHeaderRow>(), isEmpty);
        expect(
          rows.whereType<WorkbenchPinnedHeaderRow>().single.collapsed,
          isTrue,
        );
        expect(
          rows.whereType<WorkbenchWorkspaceRow>().any(
            (row) => row.isPinnedCopy,
          ),
          isFalse,
        );
      },
    );

    test('flat list without pinned workspaces has no section headers', () {
      final prefs = WorkbenchViewPrefs.defaults.copyWith(groupBy: .none);
      final rows = buildSidebarRows(_state(prefs: prefs, pinFeature: false));

      expect(rows.whereType<WorkbenchPinnedHeaderRow>(), isEmpty);
      expect(rows.whereType<WorkbenchAllHeaderRow>(), isEmpty);
      expect(rows.whereType<WorkbenchWorkspaceRow>(), hasLength(3));
    });
  });
}

final class _CountingOrder extends ListBase<String> {
  _CountingOrder(this._values);
  final List<String> _values;
  int reads = 0;

  @override
  int get length => _values.length;

  @override
  set length(int value) => throw UnsupportedError('read-only order');

  @override
  String operator [](int index) {
    reads++;
    return _values[index];
  }

  @override
  void operator []=(int index, String value) =>
      throw UnsupportedError('read-only order');
}

final class _CountingStatuses extends MapBase<String, AgentStatusEntry> {
  int reads = 0;

  @override
  AgentStatusEntry? operator [](Object? key) {
    reads++;
    return null;
  }

  @override
  Iterable<String> get keys => const [];

  @override
  void operator []=(String key, AgentStatusEntry value) =>
      throw UnsupportedError('read-only statuses');

  @override
  AgentStatusEntry? remove(Object? key) =>
      throw UnsupportedError('read-only statuses');

  @override
  void clear() => throw UnsupportedError('read-only statuses');
}
