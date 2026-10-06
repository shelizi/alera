import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_surface.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_history_graph.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';
import 'workspace_git_history_surface_test_harness.dart';

void main() {
  testWidgets(
    'file graph queries the file across all branches and renders branch lanes',
    (tester) async {
      final backend = gitHistoryBackend([
        gitHistoryCommit(
          'feature',
          parents: ['root'],
          subject: 'Feature File Change',
        ),
        gitHistoryCommit(
          'main',
          parents: ['root'],
          subject: 'Main File Change',
        ),
        gitHistoryCommit('root', parents: [], subject: 'File Created'),
      ]);
      final tab = gitHistoryTab().copyWith(
        payload: {workspaceTabGitHistoryFilePathPayloadKey: 'lib/main.dart'},
      );
      await pumpGitHistorySurface(
        tester,
        backend: backend,
        repository: GitHistoryFakeWorkbenchRepository(),
        tab: tab,
      );
      await tester.pumpAndSettle();
      final call = backend.calls.singleWhere(
        (call) => call.method == 'history',
      );
      expect(call.args['filePath'], 'lib/main.dart');
      expect(call.args['includeAllRefs'], isTrue);
      expect(find.text('lib/main.dart'), findsOneWidget);
      expect(find.text('Feature File Change'), findsOneWidget);
      expect(find.text('Main File Change'), findsOneWidget);
      expect(find.text('All Branches'), findsOneWidget);
      final graph = buildGitHistoryViewModelsFromItems(
        backend.gitHistoryResult.items,
      );
      expect(graph.any((item) => item.outputSwimlanes.length > 1), isTrue);
    },
  );

  test('collapses older merged history onto one target lane', () {
    final source = <GitHistoryItem>[
      gitHistoryCommit(
        'm5',
        parents: <String>['m4', 'f2'],
        subject: 'Newest Merge',
      ),
      gitHistoryCommit('f2', parents: <String>['f1'], subject: 'Feature 2'),
      gitHistoryCommit('f1', parents: <String>['m3'], subject: 'Feature 1'),
      gitHistoryCommit(
        'm4',
        parents: <String>['m3', 'g2'],
        subject: 'Older Merge',
      ),
      gitHistoryCommit('g2', parents: <String>['g1'], subject: 'Old Feature 2'),
      gitHistoryCommit('g1', parents: <String>['m2'], subject: 'Old Feature 1'),
      gitHistoryCommit('m3', parents: <String>['m2'], subject: 'Main 3'),
      gitHistoryCommit('m2', parents: <String>['m1'], subject: 'Main 2'),
      gitHistoryCommit('m1', parents: <String>[], subject: 'Main 1'),
    ];

    final collapsed = collapseGitHistoryOntoTargetLane(
      source,
      targetRevision: 'm5',
    );

    expect(collapsed.map((item) => item.parentIds).toList(), <List<String>>[
      <String>['f2'],
      <String>['f1'],
      <String>['m4'],
      <String>['g2'],
      <String>['g1'],
      <String>['m3'],
      <String>['m2'],
      <String>['m1'],
      <String>[],
    ]);

    final viewModels = buildGitHistoryViewModelsFromItems(collapsed);
    for (final viewModel in viewModels) {
      expect(
        viewModel.inputSwimlanes.map((node) => node.id).toSet().length,
        viewModel.inputSwimlanes.length,
      );
      expect(
        viewModel.outputSwimlanes.map((node) => node.id).toSet().length,
        viewModel.outputSwimlanes.length,
      );
      expect(viewModel.inputSwimlanes.length, lessThanOrEqualTo(1));
      expect(viewModel.outputSwimlanes.length, lessThanOrEqualTo(1));
    }
  });
  testWidgets('highlights the current HEAD row and branch ref', (tester) async {
    const currentRef = GitHistoryItemRef(
      id: 'refs/heads/main',
      name: 'main',
      revision: 'c1',
      category: GitHistoryRefCategory.branches,
    );
    final backend = FakeGitBackend()
      ..gitHistoryResult = GitHistoryResult(
        currentRef: currentRef,
        items: <GitHistoryItem>[
          gitHistoryCommit(
            'c1',
            parents: <String>[],
            subject: 'Current Commit',
            references: const <GitHistoryItemRef>[currentRef],
          ),
        ],
        hasIncomingChanges: false,
        hasOutgoingChanges: false,
        hasMore: false,
        limit: 200,
      );
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    expect(
      find.byKey(const ValueKey<String>('git-history-head-row-c1')),
      findsOneWidget,
    );
    expect(
      find.byKey(
        const ValueKey<String>('git-history-current-ref-refs/heads/main'),
      ),
      findsOneWidget,
    );
  });

  testWidgets('commit graph tab loads all-branch history into the list', (
    tester,
  ) async {
    final backend = gitHistoryBackend(<GitHistoryItem>[
      gitHistoryCommit('c2', parents: <String>['c1'], subject: 'Second Commit'),
      gitHistoryCommit(
        'c1',
        parents: <String>['c0'],
        subject: 'First Commit',
        author: 'Alera Dev',
        references: const <GitHistoryItemRef>[
          GitHistoryItemRef(id: 'refs/heads/main', name: 'main'),
        ],
      ),
    ]);
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
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
    final branchRect = tester.getRect(find.text('main'));
    final subjectRect = tester.getRect(find.text('First Commit'));
    expect(branchRect.left, lessThan(subjectRect.left));
    expect(find.byTooltip('main'), findsOneWidget);
  });
  testWidgets('scrolling to the end pages history in by offset', (
    tester,
  ) async {
    // Enough rows to overflow the viewport so the scroll listener, not the
    // short-page eager loader, drives the second request.
    final pageOne = <GitHistoryItem>[
      for (var index = 0; index < 30; index += 1)
        gitHistoryCommit(
          'page1-$index',
          parents: <String>['page1-${index + 1}'],
          subject: 'Page One $index',
        ),
    ];
    final pageTwo = <GitHistoryItem>[
      gitHistoryCommit(
        'page2-0',
        parents: <String>[],
        subject: 'Page Two Root',
      ),
    ];
    // The fake consumes queued results in call order, so page one goes first.
    final backend = FakeGitBackend()
      ..gitHistoryResultQueue.add(
        Future.value(gitHistoryResult(pageOne, hasMore: true)),
      )
      ..gitHistoryResultQueue.add(
        Future.value(gitHistoryResult(pageTwo, hasMore: false)),
      );
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
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
  testWidgets('HEAD perspective is first and roots the graph at HEAD', (
    tester,
  ) async {
    final backend =
        gitHistoryBackend(<GitHistoryItem>[
            gitHistoryCommit('c1', parents: <String>[], subject: 'Only Commit'),
          ])
          ..sourceBranches = <String>['feature', 'main', 'origin/main']
          ..headBranch = 'HEAD';
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('All Branches'));
    await tester.pumpAndSettle();

    final headItem = find.byKey(
      const ValueKey<String>('git-history-head-perspective'),
    );
    final allBranchesItem = find.widgetWithText(MenuItemButton, 'All Branches');
    expect(headItem, findsOneWidget);
    expect(find.text('HEAD'), findsOneWidget);
    expect(allBranchesItem, findsOneWidget);
    expect(
      tester.getRect(headItem).top,
      lessThan(tester.getRect(allBranchesItem).top),
    );

    await tester.tap(headItem);
    await tester.pumpAndSettle();

    final historyCalls = backend.calls
        .where((call) => call.method == 'history')
        .toList();
    expect(historyCalls, hasLength(2));
    expect(historyCalls.last.args['includeAllRefs'], isFalse);
    expect(historyCalls.last.args['baseRef'], 'HEAD');
    expect(historyCalls.last.args['offset'], isNull);
    expect(repository.tabs.single.gitHistoryAllBranches, isFalse);
    expect(repository.tabs.single.gitHistorySelectedRef, 'HEAD');
    expect(find.text('HEAD'), findsOneWidget);
    expect(
      backend.calls.where((call) => call.method == 'switchBranch'),
      isEmpty,
    );
  });

  testWidgets('branch perspective dropdown reloads history without checkout', (
    tester,
  ) async {
    final backend =
        gitHistoryBackend(<GitHistoryItem>[
            gitHistoryCommit('c1', parents: <String>[], subject: 'Only Commit'),
          ])
          ..sourceBranches = <String>['feature', 'main', 'origin/main']
          ..headBranch = 'main';
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    expect(find.text('All Branches'), findsOneWidget);
    await tester.tap(find.text('All Branches'));
    await tester.pumpAndSettle();
    expect(find.text('feature'), findsOneWidget);
    expect(find.text('main (Current)'), findsOneWidget);

    await tester.enterText(
      find.byKey(
        const ValueKey<String>('git-history-branch-perspective-search'),
      ),
      'feat',
    );
    await tester.pumpAndSettle();
    expect(find.text('feature'), findsOneWidget);
    expect(find.text('main (Current)'), findsNothing);

    await tester.tap(find.text('feature'));
    await tester.pumpAndSettle();

    final historyCalls = backend.calls
        .where((call) => call.method == 'history')
        .toList();
    expect(historyCalls, hasLength(2));
    expect(historyCalls.last.args['includeAllRefs'], isFalse);
    expect(historyCalls.last.args['baseRef'], 'feature');
    expect(historyCalls.last.args['offset'], isNull);
    expect(repository.tabs.single.gitHistoryAllBranches, isFalse);
    expect(repository.tabs.single.gitHistorySelectedRef, 'feature');
    expect(
      backend.calls.where((call) => call.method == 'switchBranch'),
      isEmpty,
    );
  });
  testWidgets('hide merged names keeps commits and graph history visible', (
    tester,
  ) async {
    const mainRef = GitHistoryItemRef(
      id: 'refs/heads/main',
      name: 'main',
      revision: 'm3',
      category: GitHistoryRefCategory.branches,
    );
    const mergedRef = GitHistoryItemRef(
      id: 'refs/heads/feature-merged',
      name: 'feature-merged',
      revision: 'f2',
      category: GitHistoryRefCategory.branches,
    );
    const openRef = GitHistoryItemRef(
      id: 'refs/heads/feature-open',
      name: 'feature-open',
      revision: 'o2',
      category: GitHistoryRefCategory.branches,
    );
    final backend = FakeGitBackend()
      ..sourceBranches = <String>['feature-merged', 'feature-open', 'main']
      ..headBranch = 'main'
      ..ancestorResults[('feature-merged', 'main')] = true
      ..ancestorResults[('feature-open', 'main')] = false
      ..gitHistoryResult = GitHistoryResult(
        currentRef: mainRef,
        items: <GitHistoryItem>[
          gitHistoryCommit(
            'm3',
            parents: <String>['m2', 'f2'],
            subject: 'Main Merge',
            references: const <GitHistoryItemRef>[mainRef],
          ),
          gitHistoryCommit(
            'o2',
            parents: <String>['o1'],
            subject: 'Open Commit 2',
            references: const <GitHistoryItemRef>[openRef],
          ),
          gitHistoryCommit(
            'o1',
            parents: <String>['m2'],
            subject: 'Open Commit 1',
          ),
          gitHistoryCommit(
            'f2',
            parents: <String>['f1'],
            subject: 'Merged Commit 2',
            references: const <GitHistoryItemRef>[mergedRef],
          ),
          gitHistoryCommit(
            'f1',
            parents: <String>['m1'],
            subject: 'Merged Commit 1',
          ),
          gitHistoryCommit('m2', parents: <String>['m1'], subject: 'Main 2'),
          gitHistoryCommit('m1', parents: <String>[], subject: 'Main 1'),
        ],
        hasIncomingChanges: false,
        hasOutgoingChanges: false,
        hasMore: false,
        limit: 200,
      );
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    expect(find.text('feature-merged'), findsOneWidget);
    expect(find.text('Merged Commit 2'), findsOneWidget);
    await tester.tap(
      find.byKey(const ValueKey<String>('git-history-merged-visibility-menu')),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Hide Merged Names'));
    await tester.pumpAndSettle();

    expect(find.text('feature-merged'), findsNothing);
    expect(find.text('feature-open'), findsOneWidget);
    expect(find.text('Merged Commit 2'), findsOneWidget);
    expect(find.text('Merged Commit 1'), findsOneWidget);
    expect(
      backend.calls.where(
        (call) =>
            call.method == 'isAncestor' &&
            call.args['ancestorRef'] == 'feature-merged' &&
            call.args['descendantRef'] == 'main',
      ),
      hasLength(1),
    );
  });

  testWidgets('hide merged graph collapses merged side history', (
    tester,
  ) async {
    const mainRef = GitHistoryItemRef(
      id: 'refs/heads/main',
      name: 'main',
      revision: 'm3',
      category: GitHistoryRefCategory.branches,
    );
    const mergedRef = GitHistoryItemRef(
      id: 'refs/heads/feature-merged',
      name: 'feature-merged',
      revision: 'f2',
      category: GitHistoryRefCategory.branches,
    );
    const openRef = GitHistoryItemRef(
      id: 'refs/heads/feature-open',
      name: 'feature-open',
      revision: 'o2',
      category: GitHistoryRefCategory.branches,
    );
    final backend = FakeGitBackend()
      ..sourceBranches = <String>['feature-merged', 'feature-open', 'main']
      ..headBranch = 'main'
      ..ancestorResults[('feature-merged', 'main')] = true
      ..ancestorResults[('feature-open', 'main')] = false
      ..gitHistoryResult = GitHistoryResult(
        currentRef: mainRef,
        items: <GitHistoryItem>[
          gitHistoryCommit(
            'm3',
            parents: <String>['m2', 'f2'],
            subject: 'Main Merge',
            references: const <GitHistoryItemRef>[mainRef],
          ),
          gitHistoryCommit(
            'o2',
            parents: <String>['o1'],
            subject: 'Open Commit 2',
            references: const <GitHistoryItemRef>[openRef],
          ),
          gitHistoryCommit(
            'o1',
            parents: <String>['m2'],
            subject: 'Open Commit 1',
          ),
          gitHistoryCommit(
            'f2',
            parents: <String>['f1'],
            subject: 'Merged Commit 2',
            references: const <GitHistoryItemRef>[mergedRef],
          ),
          gitHistoryCommit(
            'f1',
            parents: <String>['m1'],
            subject: 'Merged Commit 1',
          ),
          gitHistoryCommit('m2', parents: <String>['m1'], subject: 'Main 2'),
          gitHistoryCommit('m1', parents: <String>[], subject: 'Main 1'),
        ],
        hasIncomingChanges: false,
        hasOutgoingChanges: false,
        hasMore: false,
        limit: 200,
      );
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey<String>('git-history-merged-visibility-menu')),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Hide Merged Graph'));
    await tester.pumpAndSettle();

    expect(find.text('Merged Commit 2'), findsOneWidget);
    expect(find.text('Merged Commit 1'), findsOneWidget);
    expect(find.text('feature-merged'), findsNothing);
    expect(find.text('Open Commit 2'), findsOneWidget);
    expect(find.text('Open Commit 1'), findsOneWidget);
    expect(find.text('feature-open'), findsOneWidget);
    expect(find.text('Main Merge'), findsOneWidget);
    expect(find.text('Main 2'), findsOneWidget);
    expect(find.text('Main 1'), findsOneWidget);
  });

  testWidgets('merged target changes comparison without checkout', (
    tester,
  ) async {
    const mainRef = GitHistoryItemRef(
      id: 'refs/heads/main',
      name: 'main',
      revision: 'm1',
      category: GitHistoryRefCategory.branches,
    );
    const releaseRef = GitHistoryItemRef(
      id: 'refs/heads/release',
      name: 'release',
      revision: 'r1',
      category: GitHistoryRefCategory.branches,
    );
    final backend = FakeGitBackend()
      ..sourceBranches = <String>['feature', 'main', 'release']
      ..headBranch = 'main'
      ..ancestorResults[('feature', 'release')] = true
      ..ancestorResults[('main', 'release')] = false
      ..gitHistoryResult = GitHistoryResult(
        currentRef: mainRef,
        baseRef: releaseRef,
        items: <GitHistoryItem>[
          gitHistoryCommit(
            'm1',
            parents: <String>[],
            subject: 'Main Commit',
            references: const <GitHistoryItemRef>[mainRef],
          ),
          gitHistoryCommit(
            'r1',
            parents: <String>[],
            subject: 'Release Commit',
            references: const <GitHistoryItemRef>[releaseRef],
          ),
        ],
        hasIncomingChanges: false,
        hasOutgoingChanges: false,
        hasMore: false,
        limit: 200,
      );
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    final targetMenu = find.byKey(
      const ValueKey<String>('git-history-merged-target-menu'),
    );
    await tester.ensureVisible(targetMenu);
    await tester.tap(targetMenu);
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey<String>('git-history-merged-target-release')),
    );
    await tester.pumpAndSettle();

    final visibilityMenu = find.byKey(
      const ValueKey<String>('git-history-merged-visibility-menu'),
    );
    await tester.ensureVisible(visibilityMenu);
    await tester.tap(visibilityMenu);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Hide Merged Names'));
    await tester.pumpAndSettle();

    expect(
      backend.calls.where(
        (call) =>
            call.method == 'isAncestor' &&
            call.args['ancestorRef'] == 'feature' &&
            call.args['descendantRef'] == 'release',
      ),
      hasLength(1),
    );
    expect(
      backend.calls.where((call) => call.method == 'switchBranch'),
      isEmpty,
    );
    expect(repository.tabs.single.gitHistoryMergedIntoRef, 'release');
    expect(
      repository.tabs.single.gitHistoryMergedBranchVisibility,
      WorkspaceGitHistoryMergedBranchVisibility.hideMergedNames,
    );
  });
  testWidgets('tapping a commit opens its diff tab', (tester) async {
    final backend = gitHistoryBackend(<GitHistoryItem>[
      gitHistoryCommit('abc123', parents: <String>[], subject: 'Only Commit'),
    ]);
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
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
    mockGitHistoryClipboard(tester, (text) => copiedText = text);
    final backend = gitHistoryBackend(<GitHistoryItem>[
      gitHistoryCommit('abc123', parents: <String>[], subject: 'Only Commit'),
    ]);
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Only Commit'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();

    expect(find.text('Commit · abc123'), findsOneWidget);
    expect(find.text('Copy Commit Hash'), findsOneWidget);
    expect(find.text('Copy Commit Subject'), findsOneWidget);
    await tester.tap(find.text('Copy Commit Hash'));
    await tester.pump();

    expect(copiedText, 'abc123');
  });
  testWidgets('local branch badge switches through the workbench facade', (
    tester,
  ) async {
    final backend = gitHistoryBranchBackend();
    final controller = GitHistorySurfaceTestWorkbenchController(
      WorkbenchState(projects: <Project>[gitHistoryProject()]),
    );
    final toasts = captureAleraToasts();
    await pumpGitHistoryBranchSurface(
      tester,
      backend: backend,
      controller: controller,
    );

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
  testWidgets('remote and tag badges expose their ref actions', (tester) async {
    final backend = gitHistoryBackend(<GitHistoryItem>[
      gitHistoryCommit(
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
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('origin/main'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Branch · origin/main'), findsOneWidget);
    expect(find.text('Checkout Remote Branch'), findsOneWidget);
    expect(find.text('Delete Remote Branch'), findsOneWidget);
    expect(find.text('Pull into Current Branch'), findsOneWidget);
    expect(find.text('Copy Branch Name'), findsOneWidget);
    expect(find.text('Switch to Branch'), findsNothing);
    await tester.tapAt(const Offset(799, 399));
    await tester.pumpAndSettle();

    await tester.tap(find.text('v1.0.0'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Tag · v1.0.0'), findsOneWidget);
    expect(find.text('Push Tag'), findsOneWidget);
    expect(find.text('Delete Tag'), findsOneWidget);
    expect(find.text('Create Archive...'), findsOneWidget);
    expect(find.text('Copy Tag Name'), findsOneWidget);
    expect(find.text('Switch to Branch'), findsNothing);
  });
}

Future<void> _switchBranchFromMenu(WidgetTester tester) async {
  await tester.tap(find.text('feature'), buttons: kSecondaryMouseButton);
  await tester.pumpAndSettle();
  expect(find.text('Branch · feature'), findsOneWidget);
  expect(find.text('Switch to Branch'), findsOneWidget);
  await tester.tap(find.text('Switch to Branch'));
  await tester.pumpAndSettle();
}
