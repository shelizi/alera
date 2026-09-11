part of 'workspace_explorer_test.dart';

void _registerWorkspaceExplorerModeTests() {
  testWidgets(
    'ignored files toggle refreshes the listing without manual refresh',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _file('tracked.dart'),
        ]
        ..showAllChildrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _file('ignored.dart'),
          _file('tracked.dart'),
        ];

      await tester.pumpWidget(
        _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 320,
                height: 480,
                child: const _WorkspaceExplorerModeHarness(),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('tracked.dart'), findsOneWidget);
      expect(find.text('ignored.dart'), findsNothing);
      expect(service.listChildrenCalls.last.hideIgnored, isTrue);

      await tester.tap(find.byTooltip('Show ignored files'));
      await tester.pumpAndSettle();

      expect(find.text('tracked.dart'), findsOneWidget);
      expect(find.text('ignored.dart'), findsOneWidget);
      expect(
        service.listChildrenCalls.map((call) => call.hideIgnored),
        containsAllInOrder(<bool>[true, false]),
      );
    },
  );

  testWidgets(
    'ignored files toggle reloads expanded directories with the new filter',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _directory('src', hasChildrenHint: true),
        ]
        ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
          _file('src/main.dart'),
        ]
        ..showAllChildrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _directory('src', hasChildrenHint: true),
        ]
        ..showAllChildrenByDirectory['src'] = <native.WorkspaceFileEntry>[
          _file('src/generated.dart'),
          _file('src/main.dart'),
        ];

      await tester.pumpWidget(
        _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 320,
                height: 480,
                child: const _WorkspaceExplorerModeHarness(),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('src'));
      await tester.pumpAndSettle();
      expect(find.text('main.dart'), findsOneWidget);
      expect(find.text('generated.dart'), findsNothing);

      await tester.tap(find.byTooltip('Show ignored files'));
      await tester.pumpAndSettle();

      expect(find.text('main.dart'), findsOneWidget);
      expect(find.text('generated.dart'), findsOneWidget);
      expect(
        service.listChildrenCalls,
        contains(
          const _ListChildrenCall(relativePath: 'src', hideIgnored: false),
        ),
      );
    },
  );

  testWidgets('ignored files toggle clears stale rows when root reload fails', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('tracked.dart'),
      ]
      ..failingListChildrenCalls.add(
        const _ListChildrenCall(relativePath: '', hideIgnored: false),
      );

    await tester.pumpWidget(
      _withWorkspaceFiles(
        service,
        child: MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 320,
              height: 480,
              child: const _WorkspaceExplorerModeHarness(),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('tracked.dart'), findsOneWidget);

    await tester.tap(find.byTooltip('Show ignored files'));
    await tester.pumpAndSettle();

    expect(find.text('tracked.dart'), findsNothing);
  });

  testWidgets(
    'ignored files toggle clears stale children when expanded directory reload fails',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _directory('src', hasChildrenHint: true),
        ]
        ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
          _file('src/main.dart'),
        ]
        ..showAllChildrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _directory('src', hasChildrenHint: true),
        ]
        ..failingListChildrenCalls.add(
          const _ListChildrenCall(relativePath: 'src', hideIgnored: false),
        );

      await tester.pumpWidget(
        _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 320,
                height: 480,
                child: const _WorkspaceExplorerModeHarness(),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('src'));
      await tester.pumpAndSettle();
      expect(find.text('src'), findsOneWidget);
      expect(find.text('main.dart'), findsOneWidget);

      await tester.tap(find.byTooltip('Show ignored files'));
      await tester.pumpAndSettle();

      expect(find.text('src'), findsOneWidget);
      expect(find.text('main.dart'), findsNothing);
      expect(
        service.listChildrenCalls,
        contains(
          const _ListChildrenCall(relativePath: 'src', hideIgnored: false),
        ),
      );
    },
  );
}
