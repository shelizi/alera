import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/application/workspace_git_branch_compare_cache_provider.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_branch_compare_surface.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';

void main() {
  testWidgets(
    'branch comparison reuses queries and selection after switching tabs',
    (tester) async {
      final backend = FakeGitBackend()
        ..compareRangeResult = const GitCommitCompareResult(
          summary: GitCommitCompareSummary(
            commitOid: 'head-oid',
            parentOid: 'base-oid',
            compareRef: 'head-oid',
            baseRef: 'base-oid',
            changedFiles: 2,
            status: GitCommitCompareStatus.ready,
          ),
          entries: <GitCommitChangeEntry>[
            GitCommitChangeEntry(path: 'src/one.dart', status: .modified),
            GitCommitChangeEntry(path: 'src/two.dart', status: .added),
          ],
        )
        ..gitCommitDiffResult = const GitDiffResult(
          files: <GitDiffFile>[
            GitDiffFile(
              path: 'src/one.dart',
              area: .staged,
              status: .modified,
              lines: <GitDiffLine>[GitDiffLine.addition('+updated')],
            ),
          ],
        );
      final workspace = Workspace(
        id: 'workspace-1',
        projectId: 'project-1',
        name: 'Main',
        path: '/tmp/project',
        createdAt: DateTime.utc(2026, 10, 14),
        updatedAt: DateTime.utc(2026, 10, 14),
        kind: .main,
        status: .active,
      );
      final tab = WorkspaceTabRecord(
        id: 'branch-compare-tab',
        workspaceId: workspace.id,
        kind: .gitDiff,
        title: 'Compare main...feature/mobile',
        createdAt: DateTime.utc(2026, 10, 14),
        updatedAt: DateTime.utc(2026, 10, 14),
        payload: const <String, Object?>{
          workspaceTabGitDiffSourcePayloadKey: 'branchCompare',
          workspaceTabGitDiffScopePayloadKey: 'all',
          workspaceTabGitDiffCommitOidPayloadKey: 'head-oid',
          workspaceTabGitDiffParentOidPayloadKey: 'base-oid',
          workspaceTabGitBranchCompareBaseRefPayloadKey: 'main',
          workspaceTabGitBranchCompareHeadRefPayloadKey: 'feature/mobile',
        },
      );
      final visibleTab = ValueNotifier<WorkspaceTabRecord?>(tab);
      addTearDown(visibleTab.dispose);
      await tester.pumpWidget(
        ProviderScope(
          overrides: [gitBackendProvider.overrideWithValue(backend)],
          child: MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 1100,
                height: 700,
                child: ValueListenableBuilder<WorkspaceTabRecord?>(
                  valueListenable: visibleTab,
                  builder: (context, current, _) => current == null
                      ? const Text('Other Tab')
                      : WorkspaceGitBranchCompareSurface(
                          workspace: workspace,
                          tab: current,
                        ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('main ... feature/mobile'), findsOneWidget);
      expect(find.text('one.dart'), findsOneWidget);
      expect(find.text('two.dart'), findsOneWidget);
      expect(
        backend.calls
            .where((call) => call.method == 'compareRange')
            .single
            .args['baseRef'],
        'base-oid',
      );
      expect(
        backend.calls.where((call) => call.method == 'commitDiff'),
        isNotEmpty,
      );
      backend.gitCommitDiffResult = const GitDiffResult(
        files: [
          GitDiffFile(
            path: 'src/two.dart',
            area: .staged,
            status: .added,
            lines: [GitDiffLine.addition('+new file')],
          ),
        ],
      );
      await tester.tap(
        find.byKey(const ValueKey('branch-compare-row-src/two.dart')),
      );
      await tester.pumpAndSettle();
      final beforeSwitch = backend.calls
          .where(
            (call) => const [
              'compareRange',
              'commitDiff',
              'diffBlobBytes',
            ].contains(call.method),
          )
          .length;
      visibleTab.value = null;
      await tester.pumpAndSettle();
      expect(find.text('Other Tab'), findsOneWidget);
      visibleTab.value = tab;
      await tester.pumpAndSettle();
      expect(
        backend.calls
            .where(
              (call) => const [
                'compareRange',
                'commitDiff',
                'diffBlobBytes',
              ].contains(call.method),
            )
            .length,
        beforeSwitch,
      );
      final container = ProviderScope.containerOf(
        tester.element(find.byType(WorkspaceGitBranchCompareSurface)),
      );
      expect(
        container
            .read(workspaceGitBranchCompareCacheProvider(workspace.id, tab.id))
            .selectedPath,
        'src/two.dart',
      );

      backend.compareRangeResult = GitCommitCompareResult(
        summary: const GitCommitCompareSummary(
          commitOid: 'new-head-oid',
          parentOid: 'base-oid',
          compareRef: 'new-head-oid',
          baseRef: 'base-oid',
          changedFiles: 2,
          status: .ready,
        ),
        entries: backend.compareRangeResult.entries,
      );
      visibleTab.value = tab.copyWith(
        payload: {
          ...tab.payload,
          workspaceTabGitDiffCommitOidPayloadKey: 'new-head-oid',
        },
      );
      await tester.pumpAndSettle();
      expect(
        backend.calls.where((call) => call.method == 'compareRange').length,
        2,
      );
      expect(
        backend.calls.where((call) => call.method == 'commitDiff').length,
        3,
      );
    },
  );
}
