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
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';
import '../unit/fake_source_control_watcher.dart';

GitHistoryItem gitHistoryCommit(
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

GitHistoryResult gitHistoryResult(
  List<GitHistoryItem> items, {
  bool hasMore = false,
}) => GitHistoryResult(
  items: items,
  hasIncomingChanges: false,
  hasOutgoingChanges: false,
  hasMore: hasMore,
  limit: 200,
);

FakeGitBackend gitHistoryBackend(
  List<GitHistoryItem> items, {
  bool hasMore = false,
}) {
  return FakeGitBackend()
    ..gitHistoryResult = gitHistoryResult(items, hasMore: hasMore);
}

FakeGitBackend gitHistoryBranchBackend() => gitHistoryBackend(<GitHistoryItem>[
  gitHistoryCommit(
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

WorkspaceTabRecord gitHistoryTab() => WorkspaceTabRecord(
  id: 'tab-graph',
  workspaceId: 'workspace-1',
  kind: .gitHistory,
  title: 'Commit Graph',
  createdAt: .utc(2026, 8, 10),
  updatedAt: .utc(2026, 8, 10),
);

Project gitHistoryProject() => Project(
  id: 'project-1',
  name: 'Project',
  repoPath: '/tmp/project',
  createdAt: DateTime.utc(2026, 8, 10),
  updatedAt: DateTime.utc(2026, 8, 10),
);

Future<void> pumpGitHistorySurface(
  WidgetTester tester, {
  required FakeGitBackend backend,
  required GitHistoryFakeWorkbenchRepository repository,
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
              tab: gitHistoryTab(),
            ),
          ),
        ),
      ),
    ),
  );
}

Future<void> pumpGitHistoryBranchSurface(
  WidgetTester tester, {
  required FakeGitBackend backend,
  required WorkbenchController controller,
}) async {
  final repository = GitHistoryFakeWorkbenchRepository()
    ..tabs.add(gitHistoryTab());
  await pumpGitHistorySurface(
    tester,
    backend: backend,
    repository: repository,
    controller: controller,
  );
  await tester.pumpAndSettle();
}

void mockGitHistoryClipboard(
  WidgetTester tester,
  void Function(String) onCopy,
) {
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

List<AleraToastData> captureAleraToasts() {
  final toasts = <AleraToastData>[];
  final subscription = AleraToast.stream.listen(toasts.add);
  addTearDown(subscription.cancel);
  return toasts;
}

class GitHistorySurfaceTestWorkbenchController extends WorkbenchController {
  GitHistorySurfaceTestWorkbenchController(this._initialState);

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

class GitHistoryFakeWorkbenchRepository implements WorkbenchRepository {
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
