import 'package:alera/src/features/shell/presentation/alera_shell_page.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';

void main() {
  testWidgets(
    'right click compares target branch without switching or merging',
    (tester) async {
      final git = FakeGitBackend()
        ..sourceBranches = <String>['main', 'feature']
        ..headBranch = 'feature';
      final now = DateTime.utc(2026, 11, 2);
      final sourceWorkspace = Workspace(
        id: 'source-id',
        projectId: 'project-id',
        name: 'Source',
        path: '/repo',
        branch: 'feature',
        kind: .main,
        status: .active,
        createdAt: now,
        updatedAt: now,
      );
      final compared = <String>[];
      final switched = <String>[];
      var createdWorktrees = 0;
      await tester.pumpWidget(
        ProviderScope(
          overrides: [gitBackendProvider.overrideWithValue(git)],
          child: MaterialApp(
            home: Scaffold(
              body: Align(
                alignment: Alignment.bottomRight,
                child: WorkspaceBranchStatusBarControl(
                  workspace: sourceWorkspace,
                  sourceControlScope: WorkspaceSourceControlScope(
                    workspaceId: sourceWorkspace.id,
                    workspacePath: '/repo',
                    path: '/repo',
                  ),
                  onSwitchBranch: (branch) async => switched.add(branch),
                  onCompareWithCurrentBranch: (branch) async =>
                      compared.add(branch),
                  onCreateMergeWorkspace: (_) async {
                    createdWorktrees++;
                    throw StateError(
                      'must not create a worktree for comparison',
                    );
                  },
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('workspace-branch-status')));
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('workspace-branch-option:feature')),
        buttons: kSecondaryMouseButton,
      );
      await tester.pumpAndSettle();
      expect(
        find.byKey(
          const ValueKey('workspace-branch-compare-with-current:feature'),
        ),
        findsNothing,
      );
      await tester.tap(find.text('Copy Branch Name'));
      await tester.pumpAndSettle();

      await tester.tap(
        find.byKey(const ValueKey('workspace-branch-option:main')),
        buttons: kSecondaryMouseButton,
      );
      await tester.pumpAndSettle();
      final compareMenu = find.byKey(
        const ValueKey('workspace-branch-compare-with-current:main'),
      );
      expect(compareMenu, findsOneWidget);
      expect(
        find.byKey(const ValueKey('workspace-branch-merge-to:main')),
        findsOneWidget,
      );
      await tester.tap(compareMenu);
      await tester.pumpAndSettle();

      expect(compared, ['main']);
      expect(switched, isEmpty);
      expect(createdWorktrees, 0);
      expect(git.calls.where((call) => call.method == 'mergeRef'), isEmpty);
      expect(
        git.calls.where((call) => call.method == 'checkoutBranch'),
        isEmpty,
      );
    },
  );

  testWidgets('right click merges current branch into selected target only', (
    tester,
  ) async {
    final git = FakeGitBackend()
      ..sourceBranches = <String>['feature', 'main']
      ..headBranch = 'feature'
      ..currentBranchesByPath['/repo'] = 'feature'
      ..currentBranchesByPath['/merge/main'] = 'main'
      ..liveBranchByPath = <String, String>{'/repo': 'feature'};
    final now = DateTime.utc(2026, 11, 2);
    final sourceWorkspace = Workspace(
      id: 'source-id',
      projectId: 'project-id',
      name: 'Source',
      path: '/repo',
      branch: 'feature',
      kind: .main,
      status: .active,
      createdAt: now,
      updatedAt: now,
    );
    var creations = 0;
    var switches = 0;
    await tester.pumpWidget(
      ProviderScope(
        overrides: [gitBackendProvider.overrideWithValue(git)],
        child: MaterialApp(
          home: Scaffold(
            body: Align(
              alignment: Alignment.bottomRight,
              child: WorkspaceBranchStatusBarControl(
                workspace: sourceWorkspace,
                sourceControlScope: WorkspaceSourceControlScope(
                  workspaceId: sourceWorkspace.id,
                  workspacePath: '/repo',
                  path: '/repo',
                ),
                onSwitchBranch: (_) async => switches += 1,
                onCreateMergeWorkspace: (branch) async {
                  expect(branch, 'main');
                  creations += 1;
                  return Workspace(
                    id: 'merge-id',
                    projectId: 'project-id',
                    name: 'Merge',
                    path: '/merge/main',
                    branch: branch,
                    kind: .linked,
                    status: .active,
                    createdAt: now,
                    updatedAt: now,
                  );
                },
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byKey(const ValueKey('workspace-branch-status')));
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('workspace-branch-option:feature')),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('workspace-branch-merge-to:feature')),
      findsNothing,
    );
    await tester.tap(find.text('Copy Branch Name'));
    await tester.pumpAndSettle();

    await tester.tap(
      find.byKey(const ValueKey('workspace-branch-option:main')),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    final mergeMenu = find.byKey(
      const ValueKey('workspace-branch-merge-to:main'),
    );
    expect(mergeMenu, findsOneWidget);
    await tester.tap(mergeMenu);
    await tester.pumpAndSettle();
    expect(find.text('Merge into this branch?'), findsOneWidget);
    await tester.tap(find.text('Merge'));
    await tester.pumpAndSettle();

    expect(creations, 1);
    expect(switches, 0);
    expect(git.calls.where((call) => call.method == 'mergeRef').single.args, {
      'path': '/merge/main',
      'ref': 'feature',
    });
  });
}
