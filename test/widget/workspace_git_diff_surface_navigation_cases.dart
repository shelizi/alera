part of 'workspace_git_diff_surface_test.dart';

void _registerWorkspaceGitDiffNavigationTests() {
  testWidgets('diff starts at first change; arrows navigate both layouts', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = GitDiffResult(
        files: [
          GitDiffFile(
            path: 'lib/nav.dart',
            area: .staged,
            status: .modified,
            lines: [
              for (var i = 0; i < 75; i++) GitDiffLine.context(' before $i'),
              const GitDiffLine.hunk('@@ -76,1 +76,1 @@'),
              const GitDiffLine.deletion('-old first'),
              const GitDiffLine.addition('+new first'),
              for (var i = 0; i < 85; i++) GitDiffLine.context(' between $i'),
              const GitDiffLine.hunk('@@ -162,1 +162,1 @@'),
              const GitDiffLine.deletion('-old second'),
              const GitDiffLine.addition('+new second'),
            ],
          ),
        ],
      );
    await _pumpDiffSurface(
      tester,
      backend: backend,
      tab: _diffTab(
        filePath: 'lib/nav.dart',
        title: 'nav.dart staged',
        area: .staged,
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('1/2'), findsOneWidget);
    final unified = tester
        .widget<ListView>(
          find.byKey(const ValueKey<String>('git-diff-single-column-list')),
        )
        .controller!;
    expect(
      unified.offset,
      greaterThan(0),
      reason: 'opening a diff should reveal the first changed hunk',
    );
    final initial = unified.offset;
    await tester.tap(find.byTooltip('Next Difference'));
    await tester.pumpAndSettle();
    expect(find.text('2/2'), findsOneWidget);
    expect(unified.offset, greaterThan(initial));
    await tester.tap(find.byTooltip('Previous Difference'));
    await tester.pumpAndSettle();
    expect(find.text('1/2'), findsOneWidget);
    expect(unified.offset, lessThan(initial + 100));
    await tester.tap(find.byTooltip('Switch to Side-by-Side View'));
    await tester.pumpAndSettle();
    expect(find.text('1/2'), findsOneWidget);
    final side = tester
        .widget<ListView>(
          find.byKey(const ValueKey<String>('git-diff-side-by-side-left-list')),
        )
        .controller!;
    expect(side.offset, greaterThan(0));
    final first = side.offset;
    await tester.tap(find.byTooltip('Next Difference'));
    await tester.pumpAndSettle();
    expect(find.text('2/2'), findsOneWidget);
    expect(side.offset, greaterThan(first));
    await tester.tap(find.byTooltip('Previous Difference'));
    await tester.pumpAndSettle();
    expect(find.text('1/2'), findsOneWidget);
  });
}
