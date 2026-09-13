import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/source_control_watcher.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/application/workbench_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_surface.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';
import '../unit/fake_source_control_watcher.dart';

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
  testWidgets('secondary tapping a commit row copies its hash', (tester) async {
    String? copiedText;
    _mockClipboard(tester, (text) => copiedText = text);
    final backend = _backend(<GitHistoryItem>[
      _commit('abc123', parents: <String>[], subject: 'Only Commit'),
    ]);
    final repository = _FakeWorkbenchRepository()..tabs.add(_tab());

    await _pumpSurface(tester, backend: backend, repository: repository);
    await tester.pumpAndSettle();

    await tester.tap(find.text('Only Commit'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();

    expect(find.text('Copy Commit Hash'), findsOneWidget);
    expect(find.text('Copy Commit Subject'), findsOneWidget);
    await tester.tap(find.text('Copy Commit Hash'));
    await tester.pump();

    expect(copiedText, 'abc123');
  });
  testWidgets('local branch badge switches through the workbench facade', (
    tester,
  ) async {
    final backend = _branchBackend();
    final controller = _SurfaceTestWorkbenchController(
      WorkbenchState(projects: <Project>[_project()]),
    );
    final toasts = _captureToasts();
    await _pumpBranchSurface(tester, backend: backend, controller: controller);

    await _switchBranchFromMenu(tester);

    expect(controller.switches, hasLength(1));
    expect(controller.switches.single.branch, 'feature');
    expect(
      backend.calls.where((call) => call.method == 'history'),
      hasLength(2),
    );
    expect(
      toasts,
      contains(
        isA<AleraToastData>().having(
          (toast) => toast.message,
          'message',
          'Switched to feature',
        ),
      ),
    );
    controller.failSwitchesWith(StateError('switch failed'));
    await _switchBranchFromMenu(tester);
    expect(toasts.last.tone, AleraToastTone.error);
    expect(toasts.last.message, contains('switch failed'));
  });
  testWidgets('remote and tag badges only expose copy actions', (tester) async {
    final backend = _backend(<GitHistoryItem>[
      _commit(
        'abc123',
        parents: <String>[],
        subject: 'Only Commit',
        references: const <GitHistoryItemRef>[
          GitHistoryItemRef(
            id: 'refs/remotes/origin/main',
            name: 'origin/main',
            category: GitHistoryRefCategory.remoteBranches,
          ),
          GitHistoryItemRef(
            id: 'refs/tags/v1.0.0',
            name: 'v1.0.0',
            category: GitHistoryRefCategory.tags,
          ),
        ],
      ),
    ]);
    final repository = _FakeWorkbenchRepository()..tabs.add(_tab());

    await _pumpSurface(tester, backend: backend, repository: repository);
    await tester.pumpAndSettle();

    await tester.tap(find.text('origin/main'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Copy Branch Name'), findsOneWidget);
    expect(find.text('Switch to Branch'), findsNothing);
    await tester.tapAt(const Offset(799, 399));
    await tester.pumpAndSettle();

    await tester.tap(find.text('v1.0.0'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Copy Branch Name'), findsOneWidget);
    expect(find.text('Switch to Branch'), findsNothing);
  });
}

GitHistoryItem _commit(
  String id, {
  required List<String> parents,
  required String subject,
  String? author,
  List<GitHistoryItemRef> references = const <GitHistoryItemRef>[],
}) => GitHistoryItem(
  id: id,
  parentIds: parents,
  subject: subject,
  message: subject,
  author: author,
  timestamp: DateTime.utc(2026, 8, 10),
  references: references,
);

GitHistoryResult _historyResult(
  List<GitHistoryItem> items, {
  bool hasMore = false,
}) => GitHistoryResult(
  items: items,
  hasIncomingChanges: false,
  hasOutgoingChanges: false,
  hasMore: hasMore,
  limit: 200,
);

FakeGitBackend _backend(List<GitHistoryItem> items, {bool hasMore = false}) {
  return FakeGitBackend()
    ..gitHistoryResult = _historyResult(items, hasMore: hasMore);
}

FakeGitBackend _branchBackend() => _backend(<GitHistoryItem>[
  _commit(
    'abc123',
    parents: <String>[],
    subject: 'Feature Commit',
    references: const <GitHistoryItemRef>[
      GitHistoryItemRef(
        id: 'refs/heads/feature',
        name: 'feature',
        revision: 'abc123',
        category: GitHistoryRefCategory.branches,
      ),
    ],
  ),
]);
void _mockClipboard(WidgetTester tester, void Function(String) onCopy) {
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    SystemChannels.platform,
    (call) async {
      if (call.method == 'Clipboard.setData') {
        onCopy((call.arguments as Map<Object?, Object?>)['text'] as String);
      }
      return null;
    },
  );
  addTearDown(
    () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      null,
    ),
  );
}

List<AleraToastData> _captureToasts() {
  final toasts = <AleraToastData>[];
  final subscription = AleraToast.stream.listen(toasts.add);
  addTearDown(subscription.cancel);
  return toasts;
}

Future<void> _pumpBranchSurface(
  WidgetTester tester, {
  required FakeGitBackend backend,
  required WorkbenchController controller,
}) async {
  final repository = _FakeWorkbenchRepository()..tabs.add(_tab());
  await _pumpSurface(
    tester,
    backend: backend,
    repository: repository,
    controller: controller,
  );
  await tester.pumpAndSettle();
}

Future<void> _switchBranchFromMenu(WidgetTester tester) async {
  await tester.tap(find.text('feature'), buttons: kSecondaryMouseButton);
  await tester.pumpAndSettle();
  expect(find.text('Switch to Branch'), findsOneWidget);
  await tester.tap(find.text('Switch to Branch'));
  await tester.pumpAndSettle();
}

WorkspaceTabRecord _tab() => WorkspaceTabRecord(
  id: 'tab-graph',
  workspaceId: 'workspace-1',
  kind: .gitHistory,
  title: 'Commit Graph',
  createdAt: .utc(2026, 8, 10),
  updatedAt: .utc(2026, 8, 10),
);

Future<void> _pumpSurface(
  WidgetTester tester, {
  required FakeGitBackend backend,
  required _FakeWorkbenchRepository repository,
  WorkbenchController? controller,
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
        sourceControlWatcherProvider.overrideWithValue(
          FakeSourceControlWatcher(),
        ),
        if (controller != null)
          workbenchControllerProvider.overrideWith(() => controller),
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

Project _project() => Project(
  id: 'project-1',
  name: 'Project',
  repoPath: '/tmp/project',
  createdAt: DateTime.utc(2026, 8, 10),
  updatedAt: DateTime.utc(2026, 8, 10),
);

class _SurfaceTestWorkbenchController extends WorkbenchController {
  _SurfaceTestWorkbenchController(this._initialState);

  final WorkbenchState _initialState;
  Object? _switchError;
  final List<({Project project, Workspace workspace, String branch})> switches =
      <({Project project, Workspace workspace, String branch})>[];

  void failSwitchesWith(Object error) => _switchError = error;

  @override
  WorkbenchState build() => _initialState;

  @override
  Future<Workspace> switchWorkspaceBranch({
    required Project project,
    required Workspace workspace,
    required String branch,
  }) async {
    switches.add((project: project, workspace: workspace, branch: branch));
    if (_switchError case final Object error) {
      throw error;
    }
    return workspace.copyWith(branch: branch);
  }
}

class _FakeWorkbenchRepository implements WorkbenchRepository {
  final List<WorkspaceTabRecord> tabs = <WorkspaceTabRecord>[];
  final Map<String, WorkbenchLayout> layouts = <String, WorkbenchLayout>{};
  @override
  Future<Workspace?> findWorkspaceById(String workspaceId) async => null;
  @override
  Future<WorkspaceTabRecord?> findWorkspaceTabById(String tabId) async =>
      tabs.where((tab) => tab.id == tabId).firstOrNull;
  @override
  Future<WorkbenchLayout?> findWorkbenchLayout(String workspaceId) async =>
      layouts[workspaceId];
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
  Future<void> removeWorkspaceTab(String tabId) async =>
      tabs.removeWhere((tab) => tab.id == tabId);
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
  Future<void> removeWorkbenchLayout(String workspaceId) async =>
      layouts.remove(workspaceId);
  @override
  Future<WorkspaceTabRecord> upsertWorkspaceTab(
    WorkspaceTabRecord tab, {
    bool manualRename = false,
  }) async {
    final index = tabs.indexWhere((record) => record.id == tab.id);
    if (index == -1) tabs.add(tab);
    if (index != -1) tabs[index] = tab;
    return tab;
  }

  @override
  Future<WorkbenchLayout> upsertWorkbenchLayout(WorkbenchLayout layout) async =>
      layouts[layout.workspaceId] = layout;
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
