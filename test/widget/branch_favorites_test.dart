import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/shell/presentation/alera_shell_page.dart';
import 'package:alera/src/features/workbench/application/workbench_branch_favorites.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/application/workspace_tab_service.dart';
import 'package:alera/src/features/workbench/application/workbench_view_prefs_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_surface.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'workspace_git_history_surface_test_harness.dart';

void main() {
  testWidgets('branch status right-click copy menu and toast are localized', (
    tester,
  ) async {
    final toasts = captureAleraToasts();
    String? copied;
    mockGitHistoryClipboard(tester, (value) => copied = value);
    final backend = gitHistoryBranchBackend()
      ..sourceBranches = ['main', 'feature']
      ..headBranch = 'main';
    final workspace = Workspace(
      id: 'workspace-1',
      projectId: 'project-1',
      name: 'Main',
      path: '/tmp/project',
      createdAt: DateTime.utc(2026, 10, 10),
      updatedAt: DateTime.utc(2026, 10, 10),
      kind: .main,
      status: .active,
    );
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          gitBackendProvider.overrideWithValue(backend),
          workbenchRepositoryProvider.overrideWithValue(
            GitHistoryFakeWorkbenchRepository(),
          ),
          workbenchViewPrefsRepositoryProvider.overrideWithValue(
            _MemoryViewPrefsRepository(),
          ),
        ],
        child: MaterialApp(
          locale: const Locale('zh', 'TW'),
          supportedLocales: supportedAleraLocales,
          localizationsDelegates: const <LocalizationsDelegate<dynamic>>[
            AleraLocalizationsDelegate(),
            ...GlobalMaterialLocalizations.delegates,
          ],
          home: Scaffold(
            body: WorkspaceBranchStatusBarControl(
              workspace: workspace,
              sourceControlScope: WorkspaceSourceControlScope(
                workspaceId: workspace.id,
                workspacePath: workspace.path,
                path: workspace.path,
              ),
              onSwitchBranch: (_) async {},
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('workspace-branch-status')),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    expect(find.text('複製 Branch 名稱'), findsOneWidget);
    await tester.tap(find.text('複製 Branch 名稱'));
    await tester.pumpAndSettle();
    expect(copied, 'main');
    expect(toasts.last.message, '已複製分支名稱：main');

    await tester.tap(find.byKey(const ValueKey('workspace-branch-status')));
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<AleraIconButton>(
            find.byKey(const ValueKey('workspace-branch-favorite:feature')),
          )
          .tooltip,
      '加入我的最愛',
    );
  });

  testWidgets('status menu pins favorites and graph marks the same branch', (
    tester,
  ) async {
    final backend = gitHistoryBranchBackend()
      ..sourceBranches = ['aaa', 'feature', 'zzz']
      ..headBranch = 'aaa';
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());
    final now = DateTime.utc(2026, 10, 10);
    final workspace = Workspace(
      id: 'workspace-1',
      projectId: 'project-1',
      name: 'Main',
      path: '/tmp/project',
      createdAt: now,
      updatedAt: now,
      kind: .main,
      status: .active,
    );
    final switches = <String>[];
    final prefsRepository = _MemoryViewPrefsRepository();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          gitBackendProvider.overrideWithValue(backend),
          workbenchRepositoryProvider.overrideWithValue(repository),
          workbenchViewPrefsRepositoryProvider.overrideWithValue(
            prefsRepository,
          ),
        ],
        child: MaterialApp(
          home: Scaffold(
            body: Column(
              children: [
                Expanded(
                  child: WorkspaceGitHistorySurface(
                    workspace: workspace,
                    tab: gitHistoryTab(),
                  ),
                ),
                WorkspaceBranchStatusBarControl(
                  workspace: workspace,
                  sourceControlScope: WorkspaceSourceControlScope(
                    workspaceId: workspace.id,
                    workspacePath: workspace.path,
                    path: workspace.path,
                  ),
                  onSwitchBranch: (branch) async => switches.add(branch),
                ),
              ],
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('git-history-ref-favorite:refs/heads/feature')),
      findsOneWidget,
    );
    await tester.tap(find.byKey(const ValueKey('workspace-branch-status')));
    await tester.pumpAndSettle();
    expect(
      tester.getSize(find.byKey(const ValueKey('workspace-branch-menu'))),
      const Size(288, 300),
    );
    expect(
      tester
          .getSize(
            find.byKey(const ValueKey('workspace-branch-option:feature')),
          )
          .height,
      lessThanOrEqualTo(32),
    );
    expect(
      tester
          .widget<AleraIconButton>(
            find.byKey(const ValueKey('workspace-branch-favorite:feature')),
          )
          .minSize,
      24,
    );
    expect(
      tester
          .widget<AleraIconButton>(
            find.byKey(const ValueKey('workspace-branch-favorite:feature')),
          )
          .icon,
      Icons.star_border_rounded,
    );
    await tester.tap(
      find.byKey(const ValueKey('workspace-branch-favorite:feature')),
    );
    await tester.pumpAndSettle();
    expect(switches, isEmpty);
    expect(
      find.byKey(const ValueKey('workspace-branch-search')),
      findsOneWidget,
    );
    await tester.enterText(
      find.byKey(const ValueKey('workspace-branch-search')),
      'fea',
    );
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('workspace-branch-option:feature')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('workspace-branch-option:aaa')),
      findsNothing,
    );
    await tester.enterText(
      find.byKey(const ValueKey('workspace-branch-search')),
      '',
    );
    await tester.pumpAndSettle();
    final currentTop = tester
        .getTopLeft(find.byKey(const ValueKey('workspace-branch-option:aaa')))
        .dy;
    final favoriteTop = tester
        .getTopLeft(
          find.byKey(const ValueKey('workspace-branch-option:feature')),
        )
        .dy;
    final ordinaryTop = tester
        .getTopLeft(find.byKey(const ValueKey('workspace-branch-option:zzz')))
        .dy;
    expect(currentTop, lessThan(favoriteTop));
    expect(favoriteTop, lessThan(ordinaryTop));
    final badge = tester.widget<AleraIconButton>(
      find.byKey(const ValueKey('git-history-ref-favorite:refs/heads/feature')),
    );
    expect(badge.icon, Icons.star_rounded);
    expect(prefsRepository.prefs.favoriteBranchesByRepository.values.single, {
      'feature',
    });
    await tester.tap(find.byKey(const ValueKey('workspace-branch-status')));
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('git-history-ref-favorite:refs/heads/feature')),
    );
    await tester.pumpAndSettle();
    final context = tester.element(find.byType(WorkspaceGitHistorySurface));
    expect(
      ProviderScope.containerOf(context)
          .read(workbenchControllerProvider)
          .viewPrefs
          .favoriteBranchesByRepository,
      isEmpty,
    );
    expect(switches, isEmpty);
    expect(
      find.byKey(const ValueKey('git-history-ref-favorite:refs/heads/feature')),
      findsOneWidget,
    );
    expect(
      tester
          .widget<AleraIconButton>(
            find.byKey(
              const ValueKey('git-history-ref-favorite:refs/heads/feature'),
            ),
          )
          .icon,
      Icons.star_border_rounded,
    );
  });

  testWidgets(
    'graph favorite menu selects filtered roots and persists the filter',
    (tester) async {
      final backend = gitHistoryBranchBackend()
        ..sourceBranches = ['feature', 'main']
        ..headBranch = 'main';
      final repository = GitHistoryFakeWorkbenchRepository()
        ..tabs.add(gitHistoryTab());
      await pumpGitHistorySurface(
        tester,
        backend: backend,
        repository: repository,
        viewPrefsRepository: _MemoryViewPrefsRepository(),
      );
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('git-history-branch-perspective-menu')),
      );
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<AleraIconButton>(
              find.byKey(const ValueKey('git-history-branch-favorite:feature')),
            )
            .icon,
        Icons.star_border_rounded,
      );
      await tester.tap(
        find.byKey(const ValueKey('git-history-branch-favorite:feature')),
      );
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('git-history-favorites-only')),
        findsOneWidget,
      );
      await tester.tap(
        find.byKey(const ValueKey('git-history-favorites-only')),
      );
      await tester.pumpAndSettle();
      final call = backend.calls.lastWhere((call) => call.method == 'history');
      expect(call.args['includedRefs'], ['feature']);
      expect(repository.tabs.single.gitHistoryFavoritesOnly, isTrue);
      expect(
        backend.calls.where((call) => call.method == 'checkoutBranch'),
        isEmpty,
      );

      // Changes to the shared favorites reload an active favorites-only graph.
      backend.gitHistoryResult = gitHistoryResult([]);
      await tester.tap(
        find.byKey(
          const ValueKey('git-history-ref-favorite:refs/heads/feature'),
        ),
      );
      await tester.pumpAndSettle();
      expect(
        backend.calls
            .lastWhere((call) => call.method == 'history')
            .args['includedRefs'],
        isEmpty,
      );
      expect(find.text('No favorite branch commits'), findsOneWidget);
    },
  );

  test(
    'graph filter survives tab serialization and switching back clears it',
    () async {
      final repository = GitHistoryFakeWorkbenchRepository()
        ..tabs.add(gitHistoryTab());
      final service = WorkspaceTabService(
        repository: repository,
        now: () => DateTime.utc(2026, 10, 10),
      );
      final selected = await service.setGitHistoryAllBranches(
        tabId: 'tab-graph',
        allBranches: true,
        favoritesOnly: true,
      );
      expect(
        WorkspaceTabRecord.fromJson(selected!.toMap()).gitHistoryFavoritesOnly,
        isTrue,
      );
      final reset = await service.setGitHistoryAllBranches(
        tabId: 'tab-graph',
        allBranches: true,
      );
      expect(reset!.gitHistoryFavoritesOnly, isFalse);
      expect(reset.gitHistorySelectedRef, isNull);
      expect(
        branchFavoritesScopeKey('project-1', null),
        isNot(branchFavoritesScopeKey('project-2', null)),
      );
    },
  );
}

class _MemoryViewPrefsRepository implements WorkbenchViewPrefsRepository {
  WorkbenchViewPrefs prefs = WorkbenchViewPrefs.defaults;

  @override
  Stream<WorkbenchViewPrefs> get changes => const Stream.empty();

  @override
  Future<WorkbenchViewPrefs> load() async => prefs;

  @override
  Future<void> save(WorkbenchViewPrefs value) async {
    prefs = WorkbenchViewPrefs.fromJson(value.toMap());
  }
}
