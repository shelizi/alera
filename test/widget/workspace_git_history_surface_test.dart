import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';
import 'workspace_git_history_surface_test_harness.dart';

void main() {
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
    expect(find.text('Checkout Remote Branch'), findsOneWidget);
    expect(find.text('Delete Remote Branch'), findsOneWidget);
    expect(find.text('Pull into Current Branch'), findsOneWidget);
    expect(find.text('Copy Branch Name'), findsOneWidget);
    expect(find.text('Switch to Branch'), findsNothing);
    await tester.tapAt(const Offset(799, 399));
    await tester.pumpAndSettle();

    await tester.tap(find.text('v1.0.0'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
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
  expect(find.text('Switch to Branch'), findsOneWidget);
  await tester.tap(find.text('Switch to Branch'));
  await tester.pumpAndSettle();
}
