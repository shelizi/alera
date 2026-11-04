import 'package:alera/src/features/workbench/application/workspace_git_repositories.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/presentation/workspace_context_sidebar.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import '../unit/fake_git_backend.dart';

void main() {
  testWidgets(
    'plain folder workspace automatically focuses its first discovered Git root',
    (tester) async {
      final workspacePath = p.join('projects', 'plain-folder');
      final now = DateTime.utc(2026, 11, 4);
      final workspace = Workspace(
        id: 'workspace-folder',
        projectId: 'project-folder',
        name: 'Plain Folder',
        path: workspacePath,
        createdAt: now,
        updatedAt: now,
        kind: .main,
        status: .active,
      );
      final focused = <String>[];

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            workspaceGitRepositoriesProvider(workspacePath)
                .overrideWith((ref) async => <String>['backend', 'frontend']),
          ],
          child: MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 850,
                height: 700,
                child: WorkspaceContextSidebar(
                  workspace: workspace,
                  prefs: WorkbenchViewPrefs.defaults.copyWith(
                    activeContextPanelTab: .gitDiff,
                  ),
                  isWorkspaceGitRepository: false,
                  onToggleVisible: () {},
                  onResize: (_) {},
                  onSetContextPanelTab: (_) {},
                  onSetExplorerMode: (_) {},
                  onSetShowHiddenFiles: (_) {},
                  onSetGitDiffViewMode: (_) {},
                  onSetGitDiffGroupMode: (_) {},
                  onFocusSourceControlFolder: (relativeRoot) async {
                    focused.add(relativeRoot);
                    return true;
                  },
                  onClearSourceControlRoot: () {},
                  onOpenFile: (_) {},
                  onOpenGitDiff: ({
                    relativePath,
                    area,
                    gitDiffRoot,
                    required scope,
                    bool preview = false,
                  }) async {},
                  onOpenGitCommitDiff: ({
                    relativePath,
                    oldPath,
                    required scope,
                    gitDiffRoot,
                    required commitOid,
                    parentOid,
                    required compareRef,
                    subject,
                    message,
                    bool preview = false,
                  }) async {},
                  onOpenSearchMatch: (_) {},
                  onPathMoved: (_, _) async {},
                ),
              ),
            ),
          ),
        ),
      );

      await tester.pumpAndSettle();
      expect(focused, <String>['backend']);
      await tester.tap(
        find.byKey(const ValueKey<String>('workspace-git-repository-selector')),
      );
      await tester.pumpAndSettle();
      expect(find.text('backend'), findsOneWidget);
      expect(find.text('frontend'), findsOneWidget);
      expect(find.text('Workspace Root'), findsNothing);
    },
  );

  testWidgets(
    'multi-repository selector discovers siblings, switches and shows isolated statuses',
    (tester) async {
      final workspacePath = p.join('repo', 'multi-project');

      final now = DateTime.utc(2026, 11, 3);
      final workspace = Workspace(
        id: 'workspace-1',
        projectId: 'project-1',
        name: 'Workspace',
        path: workspacePath,
        createdAt: now,
        updatedAt: now,
        kind: .main,
        status: .active,
      );
      final git = FakeGitBackend()
        ..gitStatusResult = const GitStatusResult(
          entries: <GitChangeEntry>[
            GitChangeEntry(
              path: 'lib/main.dart',
              area: .unstaged,
              status: .modified,
            ),
          ],
        )
        ..gitRepositoryStateResult = const GitRepositoryState(
          branch: 'main',
          ahead: 2,
          behind: 1,
        );
      final focused = <String>[];
      var cleared = 0;

      Widget buildSidebar() => ProviderScope(
        overrides: [
          gitBackendProvider.overrideWithValue(git),
          workspaceGitRepositoriesProvider(workspacePath)
              .overrideWith((ref) async => <String>['', 'backend', 'frontend']),
        ],
        child: MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 850,
              height: 700,
              child: WorkspaceContextSidebar(
                workspace: workspace,
                prefs: WorkbenchViewPrefs.defaults.copyWith(
                  activeContextPanelTab: .gitDiff,
                ),
                isWorkspaceGitRepository: true,
                onToggleVisible: () {},
                onResize: (_) {},
                onSetContextPanelTab: (_) {},
                onSetExplorerMode: (_) {},
                onSetShowHiddenFiles: (_) {},
                onSetGitDiffViewMode: (_) {},
                onSetGitDiffGroupMode: (_) {},
                onFocusSourceControlFolder: (path) async {
                  focused.add(path);
                  return true;
                },
                onClearSourceControlRoot: () => cleared++,
                onOpenFile: (_) {},
                onOpenGitDiff: ({
                  relativePath,
                  area,
                  gitDiffRoot,
                  required scope,
                  bool preview = false,
                }) async {},
                onOpenGitCommitDiff: ({
                  relativePath,
                  oldPath,
                  required scope,
                  gitDiffRoot,
                  required commitOid,
                  parentOid,
                  required compareRef,
                  subject,
                  message,
                  bool preview = false,
                }) async {},
                onOpenSearchMatch: (_) {},
                onPathMoved: (_, _) async {},
              ),
            ),
          ),
        ),
      );

      await tester.pumpWidget(buildSidebar());
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey<String>('workspace-git-repository-selector')),
        findsOneWidget,
      );
      expect(find.text('3'), findsOneWidget);

      await tester.tap(
        find.byKey(const ValueKey<String>('workspace-git-repository-selector')),
      );
      await tester.pumpAndSettle();
      expect(find.text('frontend'), findsOneWidget);
      expect(find.text('backend'), findsOneWidget);
      expect(find.text('Workspace Root'), findsOneWidget);
      await tester.tap(find.text('frontend').last);
      await tester.pumpAndSettle();
      expect(focused, <String>['frontend']);

      await tester.tap(find.byTooltip('Show Git Repositories'));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey<String>('git-repo-overview:backend')),
        findsOneWidget,
      );
      expect(
        find.byKey(const ValueKey<String>('git-repo-overview:frontend')),
        findsOneWidget,
      );
      expect(find.textContaining('main · 1 files'), findsNWidgets(3));
      final queried = git.calls
          .where((call) => call.method == 'status')
          .map((call) => call.args['path'])
          .toSet();
      expect(queried, <Object?>{
        workspacePath,
        p.join(workspacePath, 'frontend'),
        p.join(workspacePath, 'backend'),
      });
      await tester.tap(
        find.byKey(const ValueKey<String>('git-repo-overview:backend')),
      );
      await tester.pumpAndSettle();
      expect(focused, <String>['frontend', 'backend']);

      await tester.tap(
        find.byKey(const ValueKey<String>('workspace-git-repository-selector')),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Workspace Root').last);
      await tester.pumpAndSettle();
      expect(cleared, 1);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();
    },
  );
}
