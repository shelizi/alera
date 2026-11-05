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
  testWidgets('right-click rename updates the selected local branch in place', (
    tester,
  ) async {
    final git = FakeGitBackend()
      ..sourceBranches = <String>['main', 'feature/old']
      ..headBranch = 'main'
      ..currentBranchesByPath['/repo'] = 'main';
    final now = DateTime.utc(2026, 11, 4);
    final workspace = Workspace(
      id: 'source-id',
      projectId: 'project-id',
      name: 'Source',
      path: '/repo',
      branch: 'main',
      kind: .main,
      status: .active,
      createdAt: now,
      updatedAt: now,
    );
    final renames = <(String, String)>[];
    var switches = 0;
    await tester.pumpWidget(
      ProviderScope(
        overrides: [gitBackendProvider.overrideWithValue(git)],
        child: MaterialApp(
          home: Scaffold(
            body: Align(
              alignment: Alignment.bottomRight,
              child: WorkspaceBranchStatusBarControl(
                workspace: workspace,
                sourceControlScope: WorkspaceSourceControlScope(
                  workspaceId: workspace.id,
                  workspacePath: '/repo',
                  path: '/repo',
                ),
                onSwitchBranch: (_) async => switches++,
                onRenameBranch: (oldBranch, newBranch) async {
                  renames.add((oldBranch, newBranch));
                  git.sourceBranches = <String>['main', newBranch];
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
      find.byKey(const ValueKey('workspace-branch-option:feature/old')),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('workspace-branch-rename:feature/old')),
    );
    await tester.pumpAndSettle();
    expect(find.text('Rename Branch'), findsOneWidget);
    await tester.enterText(find.byType(TextField).last, 'feature/new');
    await tester.tap(find.text('Rename'));
    await tester.pumpAndSettle();

    expect(renames, [('feature/old', 'feature/new')]);
    expect(switches, 0);
    expect(find.text('feature/new'), findsNothing);
    await tester.tap(find.byKey(const ValueKey('workspace-branch-status')));
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('workspace-branch-option:feature/new')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('workspace-branch-option:feature/old')),
      findsNothing,
    );
  });

  testWidgets('right-click rename accepts current branch and cancel is safe', (
    tester,
  ) async {
    final git = FakeGitBackend()
      ..sourceBranches = <String>['main', 'feature']
      ..headBranch = 'main'
      ..currentBranchesByPath['/repo'] = 'main';
    final now = DateTime.utc(2026, 11, 4);
    final workspace = Workspace(
      id: 'source-id',
      projectId: 'project-id',
      name: 'Source',
      path: '/repo',
      branch: 'main',
      kind: .main,
      status: .active,
      createdAt: now,
      updatedAt: now,
    );
    final renames = <(String, String)>[];
    await tester.pumpWidget(
      ProviderScope(
        overrides: [gitBackendProvider.overrideWithValue(git)],
        child: MaterialApp(
          home: Scaffold(
            body: Align(
              alignment: Alignment.bottomRight,
              child: WorkspaceBranchStatusBarControl(
                workspace: workspace,
                sourceControlScope: WorkspaceSourceControlScope(
                  workspaceId: workspace.id,
                  workspacePath: '/repo',
                  path: '/repo',
                ),
                onSwitchBranch: (_) async {},
                onRenameBranch: (oldBranch, newBranch) async {
                  renames.add((oldBranch, newBranch));
                  git.sourceBranches = <String>['feature', newBranch];
                  git.currentBranchesByPath['/repo'] = newBranch;
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
      find.byKey(const ValueKey('workspace-branch-option:main')),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('workspace-branch-rename:main')),
      findsOneWidget,
    );
    await tester.tap(
      find.byKey(const ValueKey('workspace-branch-rename:main')),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(renames, isEmpty);

    await tester.tap(find.byKey(const ValueKey('workspace-branch-status')));
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('workspace-branch-option:main')),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('workspace-branch-rename:main')),
    );
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).last, 'trunk');
    await tester.tap(find.text('Rename'));
    await tester.pumpAndSettle();

    expect(renames, [('main', 'trunk')]);
    expect(find.text('trunk'), findsOneWidget);
  });

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
      expect(
        git.calls.where((call) => call.method == 'createWorktree'),
        isEmpty,
      );
      expect(git.calls.where((call) => call.method == 'mergeRef'), isEmpty);
      expect(
        git.calls.where((call) => call.method == 'checkoutBranch'),
        isEmpty,
      );
    },
  );

  testWidgets(
    'right click Pull and Push target selected branch without switching',
    (tester) async {
      final git = FakeGitBackend()
        ..sourceBranches = <String>['main', 'feature']
        ..headBranch = 'main';
      final now = DateTime.utc(2026, 11, 5);
      final workspace = Workspace(
        id: 'source-id',
        projectId: 'project-id',
        name: 'Source',
        path: '/repo',
        branch: 'main',
        kind: .main,
        status: .active,
        createdAt: now,
        updatedAt: now,
      );
      var switches = 0;
      await tester.pumpWidget(
        ProviderScope(
          overrides: [gitBackendProvider.overrideWithValue(git)],
          child: MaterialApp(
            home: Scaffold(
              body: Align(
                alignment: Alignment.bottomRight,
                child: WorkspaceBranchStatusBarControl(
                  workspace: workspace,
                  sourceControlScope: WorkspaceSourceControlScope(
                    workspaceId: workspace.id,
                    workspacePath: '/repo',
                    path: '/repo',
                  ),
                  onSwitchBranch: (_) async => switches++,
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
      await tester.tap(
        find.byKey(const ValueKey('workspace-branch-pull:feature')),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('workspace-branch-status')));
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('workspace-branch-option:feature')),
        buttons: kSecondaryMouseButton,
      );
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('workspace-branch-push:feature')),
      );
      await tester.pumpAndSettle();
      expect(switches, 0);
      expect(
        git.calls.where((call) => call.method == 'pullBranch').single.args,
        {'path': '/repo', 'branch': 'feature'},
      );
      expect(
        git.calls.where((call) => call.method == 'pushBranch').single.args,
        {'path': '/repo', 'branch': 'feature'},
      );
      expect(
        git.calls.where((call) => call.method == 'checkoutBranch'),
        isEmpty,
      );
    },
  );

  testWidgets('right click merges selected branch into current checkout only', (
    tester,
  ) async {
    final git = FakeGitBackend()
      ..sourceBranches = <String>['feature', 'main']
      ..headBranch = 'main'
      ..currentBranchesByPath['/repo'] = 'main';
    final now = DateTime.utc(2026, 11, 2);
    final sourceWorkspace = Workspace(
      id: 'source-id',
      projectId: 'project-id',
      name: 'Source',
      path: '/repo',
      branch: 'main',
      kind: .main,
      status: .active,
      createdAt: now,
      updatedAt: now,
    );
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
      find.byKey(const ValueKey('workspace-branch-option:main')),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('workspace-branch-merge-to:main')),
      findsNothing,
    );
    await tester.tap(find.text('Copy Branch Name'));
    await tester.pumpAndSettle();

    await tester.tap(
      find.byKey(const ValueKey('workspace-branch-option:feature')),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    final mergeMenu = find.byKey(
      const ValueKey('workspace-branch-merge-to:feature'),
    );
    expect(mergeMenu, findsOneWidget);
    expect(find.text('Merge into Current Branch'), findsOneWidget);
    await tester.tap(mergeMenu);
    await tester.pumpAndSettle();
    expect(find.text('Merge feature into main?'), findsOneWidget);
    await tester.tap(find.text('Merge'));
    await tester.pumpAndSettle();

    expect(switches, 0);
    expect(git.calls.where((call) => call.method == 'mergeRef').single.args, {
      'path': '/repo',
      'ref': 'feature',
    });
    expect(
      git.calls.where(
        (call) =>
            call.method == 'createWorktree' ||
            call.method == 'listWorktrees' ||
            call.method == 'checkoutBranch',
      ),
      isEmpty,
    );
  });
}
