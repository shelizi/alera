import 'dart:io';

import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';
import 'workspace_git_history_surface_test_harness.dart';

void main() {
  testWidgets('modifier-click compares two commits and opens the diff tab', (
    tester,
  ) async {
    final anchor = gitHistoryCommit(
      'anchor123456',
      parents: <String>['root123'],
      subject: 'Anchor Commit',
    );
    final head = gitHistoryCommit(
      'head123456',
      parents: <String>['anchor123456'],
      subject: 'Head Commit',
    );
    final backend = _compareBackend(anchor, head)
      ..compareRangeResult = const GitCommitCompareResult(
        summary: GitCommitCompareSummary(
          commitOid: 'resolved-head',
          parentOid: 'resolved-anchor',
          compareRef: 'resolved-anchor..resolved-head',
          baseRef: 'resolved-anchor',
          changedFiles: 1,
          status: GitCommitCompareStatus.ready,
        ),
        entries: <GitCommitChangeEntry>[],
      );
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    await _tapWithModifier(tester, anchor.subject);
    expect(
      backend.calls.where((call) => call.method == 'compareRange'),
      isEmpty,
    );
    expect(_selectedAnchorRows(tester), findsOneWidget);

    await _tapWithModifier(tester, head.subject);

    final compareCalls = backend.calls
        .where((call) => call.method == 'compareRange')
        .toList();
    expect(compareCalls, hasLength(1));
    expect(compareCalls.single.args['path'], '/tmp/project');
    expect(compareCalls.single.args['baseRef'], anchor.id);
    expect(compareCalls.single.args['headRef'], head.id);
    expect(_selectedAnchorRows(tester), findsNothing);

    final diffTabs = repository.tabs
        .where((tab) => tab.kind == WorkspaceTabKind.gitDiff)
        .toList();
    expect(diffTabs, hasLength(1));
    expect(diffTabs.single.gitDiffCommitOid, 'resolved-head');
    expect(diffTabs.single.gitDiffParentOid, 'resolved-anchor');
    expect(diffTabs.single.gitDiffCompareRef, 'resolved-anchor..resolved-head');
  });

  testWidgets('plain click clears the anchor and opens a single commit diff', (
    tester,
  ) async {
    final anchor = gitHistoryCommit(
      'anchor123456',
      parents: <String>['root123'],
      subject: 'Anchor Commit',
    );
    final head = gitHistoryCommit(
      'head123456',
      parents: <String>['anchor123456'],
      subject: 'Head Commit',
    );
    final backend = _compareBackend(anchor, head);

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: GitHistoryFakeWorkbenchRepository()
        ..tabs.add(gitHistoryTab()),
    );
    await tester.pumpAndSettle();

    await _tapWithModifier(tester, anchor.subject);
    await tester.tap(find.text(head.subject));
    await tester.pumpAndSettle();

    expect(
      backend.calls.where((call) => call.method == 'compareRange'),
      isEmpty,
    );
    expect(
      backend.calls.where((call) => call.method == 'commitCompare'),
      hasLength(1),
    );
    expect(_selectedAnchorRows(tester), findsNothing);
  });
}

FakeGitBackend _compareBackend(GitHistoryItem anchor, GitHistoryItem head) {
  final items = <GitHistoryItem>[head, anchor];
  return gitHistoryBackend(items)
    ..gitHistoryResult = GitHistoryResult(
      items: items,
      hasIncomingChanges: false,
      hasOutgoingChanges: false,
      hasMore: false,
      limit: 200,
      currentRef: GitHistoryItemRef(
        id: 'refs/heads/main',
        name: 'main',
        revision: head.id,
        category: GitHistoryRefCategory.branches,
      ),
    );
}

Future<void> _tapWithModifier(WidgetTester tester, String subject) async {
  final modifier = Platform.isMacOS
      ? LogicalKeyboardKey.metaLeft
      : LogicalKeyboardKey.controlLeft;
  await tester.sendKeyDownEvent(modifier);
  await tester.tap(find.text(subject));
  await tester.sendKeyUpEvent(modifier);
  await tester.pumpAndSettle();
}

Finder _selectedAnchorRows(WidgetTester tester) {
  return find.byWidgetPredicate(
    (widget) =>
        widget is DecoratedBox &&
        widget.decoration is BoxDecoration &&
        (widget.decoration as BoxDecoration).color ==
            AleraTokens.surfaceVariant,
  );
}
