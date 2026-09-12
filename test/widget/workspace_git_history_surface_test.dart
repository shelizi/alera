import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/application/workbench_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_surface.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';

void main() {
  testWidgets('commit graph tab loads all-branch history into the list', (
    tester,
  ) async {
    final backend = _backend(<GitHistoryItem>[
      _commit('c2', parents: <String>['c1'], subject: 'Second Commit'),
      _commit(
        'c1',
        parents: <String>['c0'],
        subject: 'First Commit',
        author: 'Alera Dev',
        references: const <GitHistoryItemRef>[
          GitHistoryItemRef(id: 'refs/heads/main', name: 'main'),
        ],
      ),
    ]);
    final repository = _FakeWorkbenchRepository()..tabs.add(_tab());

    await _pumpSurface(tester, backend: backend, repository: repository);
    await tester.pumpAndSettle();

    final historyCalls = backend.calls
        .where((call) => call.method == 'history')
        .toList();
    expect(historyCalls, hasLength(1));
    expect(historyCalls.single.args['includeAllRefs'], isTrue);
    expect(historyCalls.single.args['limit'], 200);
    expect(historyCalls.single.args['offset'], isNull);
    expect(historyCalls.single.args['path'], '/tmp/project');

    expect(find.text('Commits'), findsOneWidget);
    expect(find.text('Second Commit'), findsOneWidget);
    expect(find.text('First Commit'), findsOneWidget);
    expect(find.text('main'), findsOneWidget);
    expect(find.text('Alera Dev'), findsOneWidget);
  });

  testWidgets('scrolling to the end pages history in by offset', (
    tester,
  ) async {
    // Enough rows to overflow the viewport so the scroll listener, not the
    // short-page eager loader, drives the second request.
    final pageOne = <GitHistoryItem>[
      for (var index = 0; index < 30; index += 1)
        _commit(
          'page1-$index',
          parents: <String>['page1-${index + 1}'],
          subject: 'Page One $index',
        ),
    ];
    final pageTwo = <GitHistoryItem>[
      _commit('page2-0', parents: <String>[], subject: 'Page Two Root'),
    ];
    // The fake consumes queued results in call order, so page one goes first.
    final backend = FakeGitBackend()
      ..gitHistoryResultQueue.add(
        Future.value(_historyResult(pageOne, hasMore: true)),
      )
      ..gitHistoryResultQueue.add(
        Future.value(_historyResult(pageTwo, hasMore: false)),
      );
    final repository = _FakeWorkbenchRepository()..tabs.add(_tab());

    await _pumpSurface(tester, backend: backend, repository: repository);
    await tester.pumpAndSettle();

    expect(find.text('Page One 0'), findsOneWidget);

    await tester.drag(
      find.byType(ListView),
      const Offset(0, -2000),
      warnIfMissed: false,
    );
    await tester.pumpAndSettle();

    final historyCalls = backend.calls
        .where((call) => call.method == 'history')
        .toList();
    expect(historyCalls, hasLength(2));
    expect(historyCalls.last.args['offset'], pageOne.length);
    expect(historyCalls.last.args['includeAllRefs'], isTrue);

    // The appended row sits below the lazily built viewport; scroll again to
    // bring it into view before asserting on it.
    await tester.drag(
      find.byType(ListView),
      const Offset(0, -2000),
      warnIfMissed: false,
    );
    await tester.pumpAndSettle();
    expect(find.text('Page Two Root'), findsOneWidget);
  });

  testWidgets('the All Branches toggle reloads scoped history and persists', (
    tester,
  ) async {
    final backend = _backend(<GitHistoryItem>[
      _commit('c1', parents: <String>[], subject: 'Only Commit'),
    ]);
    final repository = _FakeWorkbenchRepository()..tabs.add(_tab());

    await _pumpSurface(tester, backend: backend, repository: repository);
    await tester.pumpAndSettle();

    await tester.tap(find.text('All Branches'));
    await tester.pumpAndSettle();

    final historyCalls = backend.calls
        .where((call) => call.method == 'history')
        .toList();
    expect(historyCalls, hasLength(2));
    expect(historyCalls.last.args['includeAllRefs'], isFalse);
    expect(historyCalls.last.args['offset'], isNull);
    expect(repository.tabs.single.gitHistoryAllBranches, isFalse);
  });

  testWidgets('tapping a commit opens its diff tab', (tester) async {
    final backend = _backend(<GitHistoryItem>[
      _commit('abc123', parents: <String>[], subject: 'Only Commit'),
    ]);
    final repository = _FakeWorkbenchRepository()..tabs.add(_tab());

    await _pumpSurface(tester, backend: backend, repository: repository);
    await tester.pumpAndSettle();

    await tester.tap(find.text('Only Commit'));
    await tester.pumpAndSettle();

    expect(
      backend.calls.where((call) => call.method == 'commitCompare'),
      hasLength(1),
    );
    final diffTabs = repository.tabs
        .where((tab) => tab.kind == WorkspaceTabKind.gitDiff)
        .toList();
    expect(diffTabs, hasLength(1));
    expect(diffTabs.single.gitDiffCommitOid, 'abc123');
    expect(diffTabs.single.gitDiffCommitSubject, 'Only Commit');
  });
}

GitHistoryItem _commit(
  String id, {
  required List<String> parents,
  required String subject,
  String? author,
  List<GitHistoryItemRef> references = const <GitHistoryItemRef>[],
}) {
  return GitHistoryItem(
    id: id,
    parentIds: parents,
    subject: subject,
    message: subject,
    author: author,
    timestamp: DateTime.utc(2026, 8, 10),
    references: references,
  );
}

GitHistoryResult _historyResult(
  List<GitHistoryItem> items, {
  bool hasMore = false,
}) {
  return GitHistoryResult(
    items: items,
    hasIncomingChanges: false,
    hasOutgoingChanges: false,
    hasMore: hasMore,
    limit: 200,
  );
}

FakeGitBackend _backend(List<GitHistoryItem> items, {bool hasMore = false}) {
  return FakeGitBackend()
    ..gitHistoryResult = _historyResult(items, hasMore: hasMore);
}

WorkspaceTabRecord _tab() {
  return WorkspaceTabRecord(
    id: 'tab-graph',
    workspaceId: 'workspace-1',
    kind: .gitHistory,
    title: 'Commit Graph',
    createdAt: .utc(2026, 8, 10),
    updatedAt: .utc(2026, 8, 10),
  );
}

Future<void> _pumpSurface(
  WidgetTester tester, {
  required FakeGitBackend backend,
  required _FakeWorkbenchRepository repository,
}) {
  final workspace = Workspace(
    id: 'workspace-1',
    projectId: 'project-1',
    name: 'Main',
    path: '/tmp/project',
    createdAt: .utc(2026, 8, 10),
    updatedAt: .utc(2026, 8, 10),
    kind: .main,
    status: .active,
  );
  return tester.pumpWidget(
    ProviderScope(
      overrides: [
        gitBackendProvider.overrideWithValue(backend),
        workbenchRepositoryProvider.overrideWithValue(repository),
      ],
      child: MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 800,
            height: 400,
            child: WorkspaceGitHistorySurface(
              workspace: workspace,
              tab: _tab(),
            ),
          ),
        ),
      ),
    ),
  );
}

class _FakeWorkbenchRepository implements WorkbenchRepository {
  final List<WorkspaceTabRecord> tabs = <WorkspaceTabRecord>[];
  final Map<String, WorkbenchLayout> layouts = <String, WorkbenchLayout>{};

  @override
  Future<Workspace?> findWorkspaceById(String workspaceId) async => null;

  @override
  Future<WorkspaceTabRecord?> findWorkspaceTabById(String tabId) async {
    for (final tab in tabs) {
      if (tab.id == tabId) {
        return tab;
      }
    }
    return null;
  }

  @override
  Future<WorkbenchLayout?> findWorkbenchLayout(String workspaceId) async {
    return layouts[workspaceId];
  }

  @override
  Future<List<WorkspaceTabRecord>> listWorkspaceTabs(String workspaceId) async {
    return tabs
        .where((tab) => tab.workspaceId == workspaceId)
        .toList(growable: false);
  }

  @override
  Future<List<Workspace>> listWorkspaces(String projectId) async =>
      const <Workspace>[];

  @override
  Future<void> removeWorkspaceTab(String tabId) async {
    tabs.removeWhere((tab) => tab.id == tabId);
  }

  @override
  Future<void> removeWorkspaceTabsForWorkspace(String workspaceId) async {}

  @override
  Future<void> removeWorkspace(
    String workspaceId, {
    bool cascadeTabs = true,
  }) async {}

  @override
  Future<void> removeWorkspacesForProject(String projectId) async {}

  @override
  Future<void> removeWorkbenchLayout(String workspaceId) async {
    layouts.remove(workspaceId);
  }

  @override
  Future<WorkspaceTabRecord> upsertWorkspaceTab(
    WorkspaceTabRecord tab, {
    bool manualRename = false,
  }) async {
    final index = tabs.indexWhere((record) => record.id == tab.id);
    if (index == -1) {
      tabs.add(tab);
    } else {
      tabs[index] = tab;
    }
    return tab;
  }

  @override
  Future<WorkbenchLayout> upsertWorkbenchLayout(WorkbenchLayout layout) async {
    layouts[layout.workspaceId] = layout;
    return layout;
  }

  @override
  Future<Workspace> upsertWorkspace(Workspace workspace) async => workspace;

  @override
  Future<Workspace> setWorkspacePinned(
    String workspaceId,
    bool isPinned,
  ) async => throw StateError('Workspace not found');

  @override
  Stream<List<WorkspaceTabRecord>> watchWorkspaceTabs(String workspaceId) =>
      const Stream<List<WorkspaceTabRecord>>.empty();

  @override
  Stream<List<Workspace>> watchWorkspaces(String projectId) =>
      const Stream<List<Workspace>>.empty();
}
