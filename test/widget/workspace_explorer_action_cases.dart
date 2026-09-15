part of 'workspace_explorer_test.dart';

void _registerWorkspaceExplorerActionTests() {
  testWidgets('save all writes dirty editor documents that are not mounted', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService();
    final registry = EditorSessionRegistry();
    registry.documentFor('tab-1')
      ..attachFile(workspacePath: _workspace().path, relativePath: 'note.txt')
      ..acceptLoaded(
        native.WorkspaceEditorTextFile(
          rawContent: 'original',
          displayContent: 'original',
          contentToken: 'token-1',
          modifiedMillis: 0,
          size: .from(8),
          encoding: native.WorkspaceTextEncoding.utf8,
        ),
      )
      ..updateCurrentText('changed');

    await _pumpExplorer(tester, service, registry: registry);
    await tester.tap(find.byTooltip('Save all files'));
    await tester.pumpAndSettle();

    expect(service.writtenFiles, <String, String>{'note.txt': 'changed'});
    expect(registry.isDirty('tab-1'), isFalse);
  });

  testWidgets('background context menu creates items at workspace root', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService();
    await _pumpExplorer(tester, service);

    await tester.tapAt(const Offset(250, 220), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('New file'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'root.txt');
    await tester.tap(find.text('Create'));
    await tester.pumpAndSettle();

    expect(service.createdFiles, <String>['root.txt']);
  });

  testWidgets('copy stays internal while publishing a Windows file clipboard', (
    tester,
  ) async {
    if (!Platform.isWindows) return;
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('readme.md'),
        _directory('dest', hasChildrenHint: false),
      ];
    await _pumpExplorer(tester, service);

    await tester.tap(find.text('readme.md'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Copy'));
    await tester.pumpAndSettle();

    expect(service.systemClipboardWrites, hasLength(1));
    expect(
      service.systemClipboardWrites.single.operation,
      WorkspaceFileClipboardOperation.copy,
    );
    expect(
      service.systemClipboardWrites.single.paths.single,
      endsWith('readme.md'),
    );

    await tester.tap(find.text('dest'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Paste'));
    await tester.pumpAndSettle();

    expect(service.copiedFiles, <String>['readme.md->dest']);
    expect(service.importedClipboardCalls, isEmpty);
  });

  testWidgets('cut stays internal and clears the matching system clipboard', (
    tester,
  ) async {
    if (!Platform.isWindows) return;
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('readme.md'),
        _directory('dest', hasChildrenHint: false),
      ];
    await _pumpExplorer(tester, service);

    await tester.tap(find.text('readme.md'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cut'));
    await tester.pumpAndSettle();
    final cutSequence = service.systemClipboardSequence;

    await tester.tap(find.text('dest'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Paste'));
    await tester.pumpAndSettle();

    expect(service.movedFiles, <String>['readme.md->dest']);
    expect(service.clearedClipboardSequences, <int>[cutSequence]);
  });

  testWidgets('Explorer multi-file cut pastes into the Alera workspace root', (
    tester,
  ) async {
    if (!Platform.isWindows) return;
    final service = _FakeWorkspaceFileService()
      ..systemClipboardSequence = 42
      ..systemClipboard = const WorkspaceFileClipboardPayload(
        paths: <String>[r'C:\outside\one.txt', r'C:\outside\two.txt'],
        operation: WorkspaceFileClipboardOperation.cut,
        sequenceNumber: 42,
      );
    await _pumpExplorer(tester, service);

    await tester.tapAt(const Offset(250, 220), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Paste'));
    await tester.pumpAndSettle();

    expect(service.importedClipboardCalls, hasLength(1));
    expect(service.importedClipboardCalls.single.sourcePaths, <String>[
      r'C:\outside\one.txt',
      r'C:\outside\two.txt',
    ]);
    expect(service.importedClipboardCalls.single.targetParentRelativePath, '');
    expect(service.importedClipboardCalls.single.moveSources, isTrue);
    expect(service.clearedClipboardSequences, <int>[42]);
  });

  testWidgets('context menu copies relative paths and duplicates entries', (
    tester,
  ) async {
    String? copiedText;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copiedText =
              (call.arguments as Map<Object?, Object?>)['text'] as String?;
        }
        return null;
      },
    );
    addTearDown(() {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      );
    });
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('readme.md'),
      ];
    await _pumpExplorer(
      tester,
      service,
      workspace: _workspace(path: r'\\?\C:\repo\alera'),
    );

    await tester.tap(find.text('readme.md'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Copy path'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(copiedText, isNot(startsWith(r'\\?\')));
    expect(copiedText, contains('readme.md'));

    await tester.tap(find.text('readme.md'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Copy relative path'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(copiedText, 'readme.md');

    await tester.tap(find.text('readme.md'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Duplicate'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));

    expect(service.copiedFiles, <String>['readme.md->']);
  });

  testWidgets('context menu reveals entries in the file manager', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('readme.md'),
      ];
    final opener = _FakeWorkspaceFolderOpener();
    await _pumpExplorer(tester, service, folderOpener: opener);

    await tester.tap(find.text('readme.md'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Reveal in Finder'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));

    expect(opener.revealedPaths, <String>[p.join('/repo/alera', 'readme.md')]);
  });

  testWidgets('context menu opens Explorer files and folders in Zed', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('src', hasChildrenHint: false),
        _file('readme.md'),
      ];
    final launcher = _FakeExternalEditorLauncher();
    await _pumpExplorer(tester, service, externalEditorLauncher: launcher);

    await tester.tap(find.text('readme.md'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open in Zed'));
    await tester.pumpAndSettle();

    expect(launcher.fileRequests, hasLength(1));
    expect(launcher.fileRequests.single.workspacePath, '/repo/alera');
    expect(
      launcher.fileRequests.single.filePath,
      p.join('/repo/alera', 'readme.md'),
    );

    await tester.tap(find.text('src'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open in Zed'));
    await tester.pumpAndSettle();

    expect(launcher.workspacePaths, <String>[p.join('/repo/alera', 'src')]);
  });

  testWidgets('context menu can explicitly open a file in Alera', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('main.dart'),
      ];
    final openedInAlera = <String>[];
    await _pumpExplorer(tester, service, onOpenFileInAlera: openedInAlera.add);

    await tester.tap(find.text('main.dart'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Open in Alera'), findsOneWidget);
    await tester.tap(find.text('Open in Alera'));
    await tester.pumpAndSettle();

    expect(openedInAlera, <String>['main.dart']);
  });

  testWidgets('context menu focuses and clears source control root', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('packages', hasChildrenHint: true),
      ]
      ..childrenByDirectory['packages'] = <native.WorkspaceFileEntry>[
        _directory('packages/app', hasChildrenHint: false),
      ];
    final focused = <String>[];
    var cleared = false;

    await _pumpExplorer(
      tester,
      service,
      onFocusSourceControlFolder: (relativePath) async {
        focused.add(relativePath);
        return true;
      },
      onClearSourceControlRoot: () => cleared = true,
    );

    if (find.text('app').evaluate().isEmpty) {
      await tester.tap(find.text('packages'));
      await tester.pumpAndSettle();
    }
    await tester.tap(find.text('app'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Use As Source Control Root'));
    await tester.pumpAndSettle();

    expect(focused, <String>['packages/app']);

    await _pumpExplorer(
      tester,
      service,
      focusedSourceControlRoot: 'packages/app',
      onFocusSourceControlFolder: (relativePath) async {
        focused.add(relativePath);
        return true;
      },
      onClearSourceControlRoot: () => cleared = true,
    );
    if (find.text('app').evaluate().isEmpty) {
      await tester.tap(find.text('packages'));
      await tester.pumpAndSettle();
    }
    await tester.tap(find.text('app'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Clear Source Control Root'));
    await tester.pumpAndSettle();

    expect(cleared, isTrue);
  });

  testWidgets(
    'context menu hides source control root action without callback',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _directory('packages', hasChildrenHint: true),
        ]
        ..childrenByDirectory['packages'] = <native.WorkspaceFileEntry>[
          _directory('packages/app', hasChildrenHint: false),
        ];

      await _pumpExplorer(tester, service);

      if (find.text('app').evaluate().isEmpty) {
        await tester.tap(find.text('packages'));
        await tester.pumpAndSettle();
      }
      await tester.tap(find.text('app'), buttons: kSecondaryMouseButton);
      await tester.pumpAndSettle();

      expect(find.text('Use As Source Control Root'), findsNothing);
      expect(find.text('Clear Source Control Root'), findsNothing);
    },
  );

  testWidgets('creating a file does not use disposed state after unmount', (
    tester,
  ) async {
    final createGate = Completer<void>();
    final service = _FakeWorkspaceFileService(createGate: createGate);

    await _pumpExplorer(tester, service);
    await tester.tap(find.byTooltip('New file'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'created.dart');
    await tester.tap(find.text('Create'));
    await tester.pump();

    await tester.pumpWidget(const SizedBox.shrink());
    createGate.complete();
    await tester.pumpAndSettle();

    expect(service.createdFiles, <String>['created.dart']);
    expect(tester.takeException(), isNull);
  });
}
