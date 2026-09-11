part of 'workspace_explorer_test.dart';

void _registerWorkspaceExplorerFileTreeTests() {
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
