part of 'workspace_explorer_test.dart';

void _registerWorkspaceExplorerFileTreeTests() {
  testWidgets('Ctrl+F from Explorer tree refocuses its file filter', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('readme.md'),
      ];
    await _pumpExplorer(tester, service);
    final input = find.descendant(
      of: find.byKey(const ValueKey('workspace-explorer-filter-input')),
      matching: find.byType(TextField),
    );
    final field = tester.widget<TextField>(input);
    await tester.enterText(input, 'read');
    await tester.pumpAndSettle();

    // The file-tree background owns focus, not the filter input.
    await tester.tapAt(const Offset(220, 450));
    await tester.pump();
    expect(field.focusNode?.hasFocus, isFalse);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyF);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();
    expect(field.focusNode?.hasFocus, isTrue);
    expect(field.controller?.text, 'read');
    expect(
      field.controller?.selection,
      const TextSelection(baseOffset: 0, extentOffset: 4),
    );

    // Repeating Ctrl+F in the filter selects its query again.
    field.controller!.selection = const TextSelection.collapsed(offset: 2);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyF);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();
    expect(
      field.controller?.selection,
      const TextSelection(baseOffset: 0, extentOffset: 4),
    );
  });

  testWidgets('long Explorer names show their full text on hover', (
    tester,
  ) async {
    const filename =
        'this-is-a-very-long-filename-that-cannot-fit-in-the-explorer-panel.dart';
    const folder =
        'this-is-a-very-long-directory-name-that-cannot-fit-in-the-panel';
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file(filename),
        _directory(folder, hasChildrenHint: false),
      ];
    final opened = <String>[];
    await _pumpExplorer(tester, service, onOpenFile: opened.add);

    expect(find.byTooltip(filename), findsOneWidget);
    expect(find.byTooltip(folder), findsOneWidget);
    final fileLabel = tester.widget<Text>(find.text(filename));
    expect(fileLabel.maxLines, 1);
    expect(fileLabel.overflow, TextOverflow.ellipsis);

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(tester.getCenter(find.text(filename)));
    await tester.pump(const Duration(milliseconds: 800));
    expect(find.text(filename), findsNWidgets(2));

    await mouse.moveTo(Offset.zero);
    await tester.pumpAndSettle();
    await tester.tap(find.text(filename));
    expect(opened, <String>[filename]);
  });

  testWidgets('branch change replaces files from the previous checkout', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = [_file('old-branch.txt')];
    final workspace = _workspace().copyWith(branch: 'main');
    await _pumpExplorer(tester, service, workspace: workspace);
    expect(find.text('old-branch.txt'), findsOneWidget);
    service.childrenByDirectory[''] = [_file('new-branch.txt')];
    await _pumpExplorer(
      tester,
      service,
      workspace: workspace.copyWith(branch: 'feature'),
    );
    expect(find.text('old-branch.txt'), findsNothing);
    expect(find.text('new-branch.txt'), findsOneWidget);
  });

  testWidgets('single click toggles folders and rows expose click cursors', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('src', hasChildrenHint: true),
        _file('readme.md'),
      ]
      ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/main.dart'),
      ];
    final opened = <String>[];

    await _pumpExplorer(tester, service, onOpenFile: opened.add);

    expect(find.text('src'), findsOneWidget);
    expect(find.text('main.dart'), findsNothing);
    expect(find.byType(SvgPicture), findsNWidgets(2));
    expect(
      find.ancestor(
        of: find.text('src'),
        matching: find.byWidgetPredicate(
          (widget) =>
              widget is MouseRegion &&
              widget.cursor == SystemMouseCursors.click,
        ),
      ),
      findsOneWidget,
    );

    await tester.tap(find.text('src'));
    await tester.pumpAndSettle();
    expect(find.text('main.dart'), findsOneWidget);

    await tester.tap(find.text('src'));
    await tester.pumpAndSettle();
    expect(find.text('main.dart'), findsNothing);

    await tester.tap(find.text('readme.md'));
    await tester.pumpAndSettle();
    expect(opened, <String>['readme.md']);
  });

  testWidgets('second tap on the same file keeps it open', (tester) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('readme.md'),
      ];
    final opened = <String>[];
    final kept = <String>[];

    await _pumpExplorer(
      tester,
      service,
      onOpenFile: opened.add,
      onOpenFilePermanently: kept.add,
    );

    await tester.tap(find.text('readme.md'));
    await tester.pump();
    await tester.tap(find.text('readme.md'));
    await tester.pump();

    expect(opened, <String>['readme.md', 'readme.md']);
    expect(kept, <String>['readme.md']);
  });

  testWidgets(
    'refresh prunes stale expanded children after a folder disappears',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _directory('src', hasChildrenHint: true),
        ]
        ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
          _file('src/main.dart'),
        ];

      await _pumpExplorer(tester, service);
      await tester.tap(find.text('src'));
      await tester.pumpAndSettle();
      expect(find.text('main.dart'), findsOneWidget);

      service.childrenByDirectory[''] = const <native.WorkspaceFileEntry>[];
      await tester.tap(find.byTooltip('Refresh'));
      await tester.pumpAndSettle();

      expect(find.text('src'), findsNothing);
      expect(find.text('main.dart'), findsNothing);
    },
  );

  testWidgets(
    'refresh keeps expanded folders open and updates their contents',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _directory('src', hasChildrenHint: true),
        ]
        ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
          _file('src/old.dart'),
        ];

      await _pumpExplorer(tester, service);
      await tester.tap(find.text('src'));
      await tester.pumpAndSettle();
      expect(find.text('old.dart'), findsOneWidget);

      service.childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/new.dart'),
      ];
      await tester.tap(find.byTooltip('Refresh'));
      await tester.pumpAndSettle();

      expect(find.text('old.dart'), findsNothing);
      expect(find.text('new.dart'), findsOneWidget);
    },
  );

  testWidgets('native watcher refreshes loaded directories after changes', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('readme.md'),
      ];

    await _pumpExplorer(tester, service);
    expect(find.text('external.dart'), findsNothing);

    service.childrenByDirectory[''] = <native.WorkspaceFileEntry>[
      _file('external.dart'),
      _file('readme.md'),
    ];
    service.emitWatchBatch(<String>['']);
    await tester.pumpAndSettle();

    expect(find.text('external.dart'), findsOneWidget);
    expect(service.watchedPathUpdates.last, contains(''));
  });
}
