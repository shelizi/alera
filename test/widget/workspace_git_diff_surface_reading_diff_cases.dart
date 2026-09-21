part of 'workspace_git_diff_surface_test.dart';

void _registerWorkspaceGitDiffSurfaceReadingDiffTests() {
  testWidgets('diff surface cancels the request from the replaced tab', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/old.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
            added: 1,
            removed: 0,
          ),
        ],
      );
    final service = _BlockingReadingDiffService(backend);
    final oldTab = _diffTab(
      filePath: 'lib/old.dart',
      oldPath: 'lib/older.dart',
      title: 'old.dart',
    );

    await _pumpDiffSurface(
      tester,
      backend: backend,
      tab: oldTab,
      readingDiffService: service,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Generate Reading Diff'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Generate Reading Diff').last);
    await tester.pump();
    expect(service.started.single.filePath, 'lib/old.dart');
    expect(service.started.single.oldPath, 'lib/older.dart');

    await _pumpDiffSurface(
      tester,
      backend: backend,
      tab: _diffTab(filePath: 'lib/new.dart', title: 'new.dart'),
      readingDiffService: service,
    );
    await tester.pumpAndSettle();

    expect(service.canceled.single.filePath, 'lib/old.dart');
  });

  testWidgets(
    'unrelated tab metadata does not cancel reading diff generation',
    (tester) async {
      final backend = FakeGitBackend()
        ..gitDiffResult = const GitDiffResult(
          files: <GitDiffFile>[
            GitDiffFile(
              path: 'lib/main.dart',
              area: .unstaged,
              status: .modified,
              lines: <GitDiffLine>[GitDiffLine.addition('+new')],
              added: 1,
              removed: 1,
            ),
          ],
        );
      final service = _BlockingReadingDiffService(backend);
      final tab = _diffTab(filePath: 'lib/main.dart', title: 'main.dart');

      await _pumpDiffSurface(
        tester,
        backend: backend,
        tab: tab,
        readingDiffService: service,
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Generate Reading Diff'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Generate Reading Diff').last);
      await tester.pump();

      final reconstructed = WorkspaceTabRecord(
        id: tab.id,
        workspaceId: tab.workspaceId,
        kind: tab.kind,
        title: 'Renamed While Generating',
        createdAt: tab.createdAt,
        updatedAt: tab.updatedAt.add(const Duration(seconds: 1)),
        payload: <String, Object?>{
          ...tab.payload,
          workspaceTabManualTitlePayloadKey: true,
        },
      );
      await _pumpDiffSurface(
        tester,
        backend: backend,
        tab: reconstructed,
        readingDiffService: service,
      );
      await tester.pump();

      expect(service.canceled, isEmpty);
      expect(find.byTooltip('Cancel Reading Diff'), findsOneWidget);
      await tester.tap(find.byTooltip('Cancel Reading Diff'));
      await tester.pump();
      expect(service.canceled, hasLength(1));
    },
  );

  testWidgets('diff surface keeps reading diff failures visible', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
            added: 1,
            removed: 1,
          ),
        ],
      )
      ..readingDiffPatchResult = Uint8List.fromList(
        'diff --git a/lib/main.dart b/lib/main.dart\n'
                '--- a/lib/main.dart\n'
                '+++ b/lib/main.dart\n'
                '@@ -1 +1 @@\n'
                '-old\n'
                '+new\n'
            .codeUnits,
      );
    final readingDiffService = _PreparedReadingDiffService(backend);

    await _pumpDiffSurface(
      tester,
      backend: backend,
      readingDiffService: readingDiffService,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Generate Reading Diff'));
    await tester.pumpAndSettle();
    expect(find.text('Generate Reading Diff'), findsNWidgets(2));
    await tester.tap(find.text('Generate Reading Diff').last);
    await tester.pumpAndSettle();

    expect(find.text('Reading diff generation failed'), findsOneWidget);
    expect(
      find.text('Codex failed: Invalid schema for response format.'),
      findsOneWidget,
    );
  });

  testWidgets('diff surface keeps cancel visible when AI Assist is disabled', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
            added: 1,
            removed: 1,
          ),
        ],
      );
    final service = _BlockingReadingDiffService(backend);
    final settingsController = _MutableSettingsController(.defaults);
    await _pumpDiffSurface(
      tester,
      backend: backend,
      readingDiffService: service,
      settingsController: settingsController,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Generate Reading Diff'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Generate Reading Diff').last);
    await tester.pump();

    settingsController.setAiAssistEnabled(false);
    await tester.pump();

    expect(find.byTooltip('Cancel Reading Diff'), findsOneWidget);
    await tester.tap(find.byTooltip('Cancel Reading Diff'));
    await tester.pump();
    expect(service.canceled, hasLength(1));
  });

  testWidgets('diff surface cancels before preparation opens confirmation', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
            added: 1,
            removed: 1,
          ),
        ],
      );
    final service = _BlockingPreparationReadingDiffService(backend);
    await _pumpDiffSurface(
      tester,
      backend: backend,
      readingDiffService: service,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Generate Reading Diff'));
    await tester.pump();
    expect(find.byTooltip('Cancel Reading Diff'), findsOneWidget);

    await tester.tap(find.byTooltip('Cancel Reading Diff'));
    await tester.pump();
    expect(service.canceled, hasLength(1));

    service.completePreparation();
    await tester.pumpAndSettle();

    expect(find.text('Generate Reading Diff'), findsNothing);
    expect(find.byTooltip('Generate Reading Diff'), findsOneWidget);
  });

  testWidgets('diff surface blocks retry until cancellation completes', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
            added: 1,
            removed: 1,
          ),
        ],
      );
    final service = _BlockingReadingDiffService(
      backend,
      completeOnCancel: false,
    );
    await _pumpDiffSurface(
      tester,
      backend: backend,
      readingDiffService: service,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Generate Reading Diff'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Generate Reading Diff').last);
    await tester.pump();

    await tester.tap(find.byTooltip('Cancel Reading Diff'));
    await tester.pump();

    expect(service.canceled, hasLength(1));
    expect(find.byTooltip('Cancel Reading Diff'), findsOneWidget);
    expect(find.byTooltip('Generate Reading Diff'), findsNothing);

    service.completeCancellation();
    await tester.pumpAndSettle();

    expect(find.byTooltip('Generate Reading Diff'), findsOneWidget);
  });

  testWidgets('diff refresh waits for reading diff cancellation', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
            added: 1,
            removed: 1,
          ),
        ],
      );
    final service = _BlockingReadingDiffService(
      backend,
      completeOnCancel: false,
    );
    await _pumpDiffSurface(
      tester,
      backend: backend,
      readingDiffService: service,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Generate Reading Diff'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Generate Reading Diff').last);
    await tester.pump();

    await tester.tap(find.byTooltip('Refresh'));
    await tester.pump();

    expect(service.canceled, hasLength(1));
    expect(find.byTooltip('Cancel Reading Diff'), findsOneWidget);
    expect(find.byTooltip('Generate Reading Diff'), findsNothing);

    service.completeCancellation();
    await tester.pumpAndSettle();

    expect(find.byTooltip('Generate Reading Diff'), findsOneWidget);
  });

  testWidgets(
    'diff surface keeps original toggle after AI Assist is disabled',
    (tester) async {
      final backend = FakeGitBackend()
        ..gitDiffResult = const GitDiffResult(
          files: <GitDiffFile>[
            GitDiffFile(
              path: 'lib/main.dart',
              area: .unstaged,
              status: .modified,
              lines: <GitDiffLine>[GitDiffLine.addition('+new')],
              added: 1,
              removed: 1,
            ),
          ],
        );
      final controller = _GitDiffSurfaceTestController(
        initialViewPrefs: WorkbenchViewPrefs.defaults.copyWith(
          gitDiffContentMode: GitDiffContentMode.diffOnly,
        ),
      );
      final settingsController = _MutableSettingsController(.defaults);
      await _pumpDiffSurface(
        tester,
        backend: backend,
        controller: controller,
        readingDiffService: _CachedReadingDiffService(backend),
        settingsController: settingsController,
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Generate Reading Diff'));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Show Original Diff'), findsOneWidget);

      settingsController.setAiAssistEnabled(false);
      await tester.pump();

      expect(find.byTooltip('Show Original Diff'), findsOneWidget);
      expect(find.byTooltip('Regenerate Reading Diff'), findsNothing);
      await tester.tap(find.byTooltip('Show Original Diff'));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Show Reading Diff'), findsOneWidget);
      expect(find.text('+new'), findsOneWidget);
      expect(find.text('+snapshot-value'), findsNothing);
    },
  );

  testWidgets('reading diff survives presentation and content mode changes', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
            added: 1,
            removed: 1,
          ),
        ],
      )
      ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: true)] =
          Uint8List.fromList('old\n'.codeUnits)
      ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: false)] =
          Uint8List.fromList('new\n'.codeUnits);
    final controller = _GitDiffSurfaceTestController(
      initialViewPrefs: WorkbenchViewPrefs.defaults.copyWith(
        gitDiffContentMode: GitDiffContentMode.diffOnly,
      ),
    );
    final service = _CachedReadingDiffService(backend);
    await _pumpDiffSurface(
      tester,
      backend: backend,
      controller: controller,
      readingDiffService: service,
      tab: _diffTab(filePath: 'lib/main.dart', title: 'main.dart'),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Generate Reading Diff'));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Show Original Diff'), findsOneWidget);

    await tester.tap(find.byTooltip('Show Original Diff'));
    await tester.pumpAndSettle();
    expect(find.text('+new'), findsOneWidget);

    await tester.tap(find.byTooltip('Switch to Side-by-Side View'));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Switch to Single-Column View'), findsOneWidget);
    expect(find.byTooltip('Show Reading Diff'), findsOneWidget);

    await tester.tap(find.byTooltip('Switch to Full File View'));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Switch to Diff Only'), findsOneWidget);
    expect(find.byTooltip('Show Reading Diff'), findsOneWidget);

    await tester.tap(find.byTooltip('Show Reading Diff'));
    await tester.pumpAndSettle();
    expect(find.text('Overview'), findsOneWidget);
    expect(find.text('Update the value.'), findsOneWidget);
  });

  testWidgets(
    'diff mode switches do not cancel active reading diff generation',
    (tester) async {
      final backend = FakeGitBackend()
        ..gitDiffResult = const GitDiffResult(
          files: <GitDiffFile>[
            GitDiffFile(
              path: 'lib/main.dart',
              area: .unstaged,
              status: .modified,
              lines: <GitDiffLine>[GitDiffLine.addition('+new')],
              added: 1,
              removed: 1,
            ),
          ],
        )
        ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: true)] =
            Uint8List.fromList('old\n'.codeUnits)
        ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: false)] =
            Uint8List.fromList('new\n'.codeUnits);
      final controller = _GitDiffSurfaceTestController(
        initialViewPrefs: WorkbenchViewPrefs.defaults.copyWith(
          gitDiffContentMode: GitDiffContentMode.diffOnly,
        ),
      );
      final service = _BlockingReadingDiffService(backend);
      await _pumpDiffSurface(
        tester,
        backend: backend,
        controller: controller,
        readingDiffService: service,
        tab: _diffTab(filePath: 'lib/main.dart', title: 'main.dart'),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Generate Reading Diff'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Generate Reading Diff').last);
      await tester.pump();
      expect(find.byTooltip('Cancel Reading Diff'), findsOneWidget);

      await tester.tap(find.byTooltip('Switch to Side-by-Side View'));
      await tester.pump();
      expect(service.canceled, isEmpty);
      expect(find.byTooltip('Cancel Reading Diff'), findsOneWidget);

      await tester.tap(find.byTooltip('Switch to Full File View'));
      await tester.pump();
      await tester.pump();
      expect(service.canceled, isEmpty);
      expect(find.byTooltip('Cancel Reading Diff'), findsOneWidget);

      await tester.tap(find.byTooltip('Cancel Reading Diff'));
      await tester.pumpAndSettle();
      expect(service.canceled, hasLength(1));
    },
  );

  testWidgets('failed regeneration preserves the prior reading diff result', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
            added: 1,
            removed: 1,
          ),
        ],
      );
    final controller = _GitDiffSurfaceTestController(
      initialViewPrefs: WorkbenchViewPrefs.defaults.copyWith(
        gitDiffContentMode: GitDiffContentMode.diffOnly,
      ),
    );
    final service = _RegenerationFailureReadingDiffService(backend);
    await _pumpDiffSurface(
      tester,
      backend: backend,
      controller: controller,
      readingDiffService: service,
      tab: _diffTab(filePath: 'lib/main.dart', title: 'main.dart'),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Generate Reading Diff'));
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Regenerate Reading Diff'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Generate Reading Diff').last);
    await tester.pumpAndSettle();
    expect(find.text('Reading diff generation failed'), findsOneWidget);

    await tester.tap(find.byTooltip('Show Original Diff'));
    await tester.pumpAndSettle();
    expect(find.text('+new'), findsOneWidget);
    expect(find.text('+snapshot-value'), findsNothing);
    expect(find.text('+replacement-snapshot'), findsNothing);

    await tester.tap(find.byTooltip('Show Reading Diff'));
    await tester.pumpAndSettle();
    expect(find.text('Overview'), findsOneWidget);
    expect(find.text('Update the value.'), findsOneWidget);
  });

  testWidgets('reading diff generation supports all-changes scope', (
    tester,
  ) async {
    const file = GitDiffFile(
      path: 'lib/main.dart',
      area: .unstaged,
      status: .modified,
      lines: <GitDiffLine>[GitDiffLine.addition('+new')],
      added: 1,
      removed: 1,
    );
    final backend = FakeGitBackend()
      ..gitStatusResult = const GitStatusResult(
        entries: <GitChangeEntry>[
          GitChangeEntry(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
          ),
        ],
      )
      ..gitDiffAllPageResult = const GitDiffPage(files: <GitDiffFile>[file]);
    final service = _CachedReadingDiffService(backend);
    await _pumpDiffSurface(
      tester,
      backend: backend,
      readingDiffService: service,
      tab: _diffTab(
        scope: .all,
        filePath: null,
        area: null,
        title: 'all changes',
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Generate Reading Diff'));
    await tester.pumpAndSettle();

    expect(service.prepared, hasLength(1));
    expect(service.prepared.single.filePath, isNull);
    expect(service.prepared.single.oldPath, isNull);
    expect(service.prepared.single.area, isNull);
    expect(find.byTooltip('Show Original Diff'), findsOneWidget);
  });
}
