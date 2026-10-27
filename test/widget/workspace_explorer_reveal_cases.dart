part of 'workspace_explorer_test.dart';

void _registerWorkspaceExplorerRevealTests() {
  testWidgets('queued reveal expands ancestors and selects the file', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('src', hasChildrenHint: true),
      ]
      ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/main.dart'),
      ];
    late ProviderContainer container;
    await tester.pumpWidget(
      _withWorkspaceFiles(
        service,
        child: MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 320,
              height: 480,
              child: Consumer(
                builder: (context, ref, _) {
                  container = ProviderScope.containerOf(context);
                  return WorkspaceExplorer(
                    workspace: _workspace(),
                    mode: .hideIgnored,
                    onModeChanged: (_) {},
                    showHiddenFiles: false,
                    onShowHiddenFilesChanged: (_) {},
                    onOpenFile: (_) {},
                    onPathMoved: (_, _) async {},
                  );
                },
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('main.dart'), findsNothing);

    container
        .read(workspaceExplorerRevealControllerProvider.notifier)
        .reveal(workspaceId: 'workspace-1', relativePath: 'src/main.dart');
    await tester.pumpAndSettle();

    expect(find.text('main.dart'), findsOneWidget);
    expect(container.read(workspaceExplorerRevealControllerProvider), isNull);
  });

  testWidgets(
    'opening a fuzzy search result reveals and persists expanded ancestors on workspace switches',
    (tester) async {
      final first = _workspace();
      final second = _workspace(
        id: 'workspace-2',
        path: '/repo/other',
      ).copyWith(projectId: 'project-2');
      final service = _FakeWorkspaceFileService()
        ..childrenByWorkspacePath[first.path] =
            <String, List<native.WorkspaceFileEntry>>{
              '': <native.WorkspaceFileEntry>[
                _directory('src', hasChildrenHint: true),
              ],
              'src': <native.WorkspaceFileEntry>[
                _directory('src/deep', hasChildrenHint: true),
              ],
              'src/deep': <native.WorkspaceFileEntry>[
                _file('src/deep/my_special.dart'),
              ],
            }
        ..childrenByWorkspacePath[second.path] =
            <String, List<native.WorkspaceFileEntry>>{
              '': <native.WorkspaceFileEntry>[_file('other.dart')],
            }
        ..quickOpenEntries = <String>['src/deep/my_special.dart'];
      final opened = <String>[];
      await tester.pumpWidget(
        _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 330,
                height: 520,
                child: _SwitchingWorkspaceExplorerHarness(
                  first: first,
                  second: second,
                  onOpenFile: opened.add,
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('my_special.dart'), findsNothing);

      await tester.enterText(find.byType(TextField), 'sdmy');
      await tester.pumpAndSettle();
      expect(find.text('src/deep/my_special.dart'), findsOneWidget);

      await tester.tap(find.text('src/deep/my_special.dart'));
      await tester.pumpAndSettle();
      expect(opened, <String>['src/deep/my_special.dart']);
      expect(find.text('Filter files...'), findsOneWidget);
      // Preserve the search results for Ctrl+A and multi-selection until
      // the user explicitly clears the filter.
      expect(find.text('src/deep/my_special.dart'), findsOneWidget);
      await tester.enterText(find.byType(TextField), '');
      await tester.pumpAndSettle();
      expect(find.text('src/deep/my_special.dart'), findsNothing);
      expect(find.text('src'), findsOneWidget);
      expect(find.text('deep'), findsOneWidget);
      expect(find.text('my_special.dart'), findsOneWidget);

      await tester.tap(find.text('Switch workspace'));
      await tester.pumpAndSettle();
      expect(find.text('other.dart'), findsOneWidget);
      expect(find.text('my_special.dart'), findsNothing);

      await tester.tap(find.text('Switch workspace'));
      await tester.pumpAndSettle();
      expect(find.text('my_special.dart'), findsOneWidget);
      expect(opened, hasLength(1));
    },
  );

  testWidgets(
    'opening a folder found by search persists that folder expansion across projects',
    (tester) async {
      final first = _workspace();
      final second = _workspace(
        id: 'workspace-2',
        path: '/repo/other',
      ).copyWith(projectId: 'project-2');
      final service = _FakeWorkspaceFileService()
        ..childrenByWorkspacePath[first
            .path] = <String, List<native.WorkspaceFileEntry>>{
          '': <native.WorkspaceFileEntry>[
            _directory('src', hasChildrenHint: true),
          ],
          'src': <native.WorkspaceFileEntry>[
            _directory('src/deep', hasChildrenHint: true),
          ],
          'src/deep': <native.WorkspaceFileEntry>[_file('src/deep/child.dart')],
        }
        ..childrenByWorkspacePath[second.path] =
            <String, List<native.WorkspaceFileEntry>>{
              '': <native.WorkspaceFileEntry>[_file('other.dart')],
            }
        ..quickOpenDirectories = <String>['src/deep'];
      await tester.pumpWidget(
        _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 330,
                height: 520,
                child: _SwitchingWorkspaceExplorerHarness(
                  first: first,
                  second: second,
                  onOpenFile: (_) {},
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField), 'sd');
      await tester.pumpAndSettle();
      expect(find.text('src/deep'), findsOneWidget);
      await tester.tap(find.text('src/deep'));
      await tester.pumpAndSettle();
      expect(find.text('child.dart'), findsOneWidget);
      await tester.tap(find.text('Switch workspace'));
      await tester.pumpAndSettle();
      expect(find.text('child.dart'), findsNothing);
      await tester.tap(find.text('Switch workspace'));
      await tester.pumpAndSettle();
      expect(find.text('child.dart'), findsOneWidget);
    },
  );

  testWidgets(
    'queued Explorer reveal retains automatic folder expansion after switching workspaces',
    (tester) async {
      final first = _workspace();
      final second = _workspace(
        id: 'workspace-2',
        path: '/repo/other',
      ).copyWith(projectId: 'project-2');
      final service = _FakeWorkspaceFileService()
        ..childrenByWorkspacePath[first
            .path] = <String, List<native.WorkspaceFileEntry>>{
          '': <native.WorkspaceFileEntry>[
            _directory('lib', hasChildrenHint: true),
          ],
          'lib': <native.WorkspaceFileEntry>[
            _directory('lib/src', hasChildrenHint: true),
          ],
          'lib/src': <native.WorkspaceFileEntry>[_file('lib/src/needle.dart')],
        }
        ..childrenByWorkspacePath[second.path] =
            <String, List<native.WorkspaceFileEntry>>{
              '': <native.WorkspaceFileEntry>[_file('other.dart')],
            };
      late ProviderContainer container;
      await tester.pumpWidget(
        _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 330,
                height: 520,
                child: Consumer(
                  builder: (context, ref, _) {
                    container = ProviderScope.containerOf(context);
                    return _SwitchingWorkspaceExplorerHarness(
                      first: first,
                      second: second,
                      onOpenFile: (_) {},
                    );
                  },
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      container
          .read(workspaceExplorerRevealControllerProvider.notifier)
          .reveal(workspaceId: first.id, relativePath: 'lib/src/needle.dart');
      await tester.pumpAndSettle();
      expect(find.text('needle.dart'), findsOneWidget);

      await tester.tap(find.text('Switch workspace'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Switch workspace'));
      await tester.pumpAndSettle();
      expect(find.text('needle.dart'), findsOneWidget);
    },
  );
}
