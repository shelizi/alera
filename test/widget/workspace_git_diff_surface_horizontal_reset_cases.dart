part of 'workspace_git_diff_surface_test.dart';

void _registerWorkspaceGitDiffHorizontalResetTests() {
  testWidgets('switching Git Diff files resets x offset despite long lines', (
    tester,
  ) async {
    final longA = List<String>.filled(310, 'a').join();
    final longB = List<String>.filled(400, 'b').join();
    GitDiffResult resultFor(String path, String content) => GitDiffResult(
      files: [
        GitDiffFile(
          path: path,
          area: .staged,
          status: .modified,
          lines: [
            const GitDiffLine.hunk('@@ -1,1 +1,1 @@'),
            const GitDiffLine.deletion('-old line'),
            GitDiffLine.addition('+$content'),
          ],
        ),
      ],
    );
    final backend = FakeGitBackend();
    for (final (path, content) in [
      ('lib/first.dart', longA),
      ('lib/second.dart', longB),
    ]) {
      backend.diffBlobBytesBySide[(filePath: path, oldSide: true)] =
          Uint8List.fromList('old line\n'.codeUnits);
      backend.diffBlobBytesBySide[(filePath: path, oldSide: false)] =
          Uint8List.fromList('$content\n'.codeUnits);
    }
    Future<ScrollController> show(String path, String content) async {
      backend.gitDiffResult = resultFor(path, content);
      await _pumpDiffSurface(
        tester,
        backend: backend,
        tab: _diffTab(filePath: path, title: path, area: .staged),
      );
      await tester.pumpAndSettle();
      final controller = tester
          .widget<Scrollbar>(
            find.byKey(
              const ValueKey<String>('git-diff-single-column-x-scrollbar'),
            ),
          )
          .controller!;
      expect(controller.hasClients, isTrue);
      expect(controller.position.maxScrollExtent, greaterThan(100));
      return controller;
    }

    final first = await show('lib/first.dart', longA);
    first.jumpTo(first.position.maxScrollExtent);
    await tester.pump();
    expect(first.offset, greaterThan(100));
    final second = await show('lib/second.dart', longB);
    expect(second.offset, closeTo(0, 0.5));
    second.jumpTo(second.position.maxScrollExtent);
    await tester.pump();
    final returned = await show('lib/first.dart', longA);
    expect(returned.offset, closeTo(0, 0.5));
  });

  testWidgets('editable side-by-side starts new file at x=0', (tester) async {
    final longLine = List<String>.filled(290, 'w').join();
    final content = '$longLine\nshort\n';
    GitDiffResult resultFor(String path) => GitDiffResult(
      files: [
        GitDiffFile(
          path: path,
          area: .unstaged,
          status: .modified,
          lines: const [
            GitDiffLine.hunk('@@ -1,1 +1,1 @@'),
            GitDiffLine.deletion('-old line'),
            GitDiffLine.addition('+new line'),
          ],
        ),
      ],
    );
    final backend = FakeGitBackend()
      ..gitDiffResult = resultFor('lib/first.dart');
    for (final path in ['lib/first.dart', 'lib/second.dart']) {
      backend.diffBlobBytesBySide[(filePath: path, oldSide: true)] =
          Uint8List.fromList(content.codeUnits);
      backend.diffBlobBytesBySide[(filePath: path, oldSide: false)] =
          Uint8List.fromList(content.codeUnits);
    }
    final files = _EditableDiffFileService(content: content);
    await _pumpDiffSurface(
      tester,
      backend: backend,
      workspaceFileService: files,
      tab: _diffTab(filePath: 'lib/first.dart', title: 'first.dart'),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Switch to Side-by-Side View'));
    await tester.pumpAndSettle();
    final firstX = tester
        .widget<Scrollbar>(
          find.byKey(
            const ValueKey<String>(
              'git-diff-working-tree-editor-x-scrollbar-lib/first.dart',
            ),
          ),
        )
        .controller!;
    firstX.jumpTo(160);
    await tester.pump();
    expect(firstX.offset, closeTo(160, 0.5));
    backend.gitDiffResult = resultFor('lib/second.dart');
    await _pumpDiffSurface(
      tester,
      backend: backend,
      workspaceFileService: files,
      tab: _diffTab(filePath: 'lib/second.dart', title: 'second.dart'),
    );
    await tester.pumpAndSettle();
    final secondX = tester
        .widget<Scrollbar>(
          find.byKey(
            const ValueKey<String>(
              'git-diff-working-tree-editor-x-scrollbar-lib/second.dart',
            ),
          ),
        )
        .controller!;
    expect(
      secondX.offset,
      closeTo(0, 0.5),
      reason: 'editable pane must discard previous file x offset',
    );
  });
}
