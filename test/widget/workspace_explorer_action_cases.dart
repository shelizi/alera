part of 'workspace_explorer_test.dart';

void _registerWorkspaceExplorerActionTests() {
  testWidgets('filtered unloaded file gets the normal complete file menu', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('lib', hasChildrenHint: true),
      ]
      ..quickOpenEntries = <String>['lib/deep/hidden_match.dart'];
    final launcher = _FakeExternalEditorLauncher();
    final openedInAlera = <String>[];
    await _pumpExplorer(
      tester,
      service,
      externalEditorLauncher: launcher,
      onOpenFileInAlera: openedInAlera.add,
    );

    await tester.enterText(find.byType(TextField), 'hidden_match');
    await tester.pumpAndSettle();
    expect(find.text('lib/deep/hidden_match.dart'), findsOneWidget);
    expect(
      service.listChildrenCalls.where((call) => call.relativePath == 'lib'),
      isEmpty,
    );

    await tester.tap(
      find.text('lib/deep/hidden_match.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    for (final label in <String>[
      'New file',
      'New folder',
      'Copy',
      'Cut',
      'Paste',
      'Copy path',
      'Copy relative path',
      'Duplicate',
    ]) {
      expect(find.text(label), findsOneWidget, reason: label);
    }
    expect(find.text('Use As Source Control Root'), findsNothing);
    expect(find.text('Clear Source Control Root'), findsNothing);

    // This menu is taller than the 480px test window. Scroll its own overlay
    // to verify the file-only actions rather than treating unbuilt rows as
    // missing menu entries.
    await tester.scrollUntilVisible(
      find.text('Open in Alera'),
      96,
      scrollable: find.byType(Scrollable).last,
    );
    expect(find.text('Open in Zed'), findsOneWidget);
    expect(find.text('Open in Alera'), findsOneWidget);
    await tester.tap(find.text('Open in Alera'));
    await tester.pumpAndSettle();
    expect(openedInAlera, <String>['lib/deep/hidden_match.dart']);
  });

  testWidgets('filtered unloaded result copies its actual relative path', (
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
      ..quickOpenEntries = <String>['lib/deep/found.dart'];
    final opener = _FakeWorkspaceFolderOpener();
    await _pumpExplorer(tester, service, folderOpener: opener);
    await tester.enterText(find.byType(TextField), 'found');
    await tester.pumpAndSettle();

    await tester.tap(
      find.text('lib/deep/found.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Copy relative path'));
    await tester.pumpAndSettle();
    expect(copiedText, 'lib/deep/found.dart');

    await tester.tap(
      find.text('lib/deep/found.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open with Default Application'));
    await tester.pumpAndSettle();
    expect(opener.defaultOpenedPaths, <String>[
      p.join('/repo/alera', 'lib', 'deep', 'found.dart'),
    ]);
  });

  testWidgets('filtered Ctrl multi-selection copies only chosen files', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..quickOpenEntries = <String>[
        'src/one.dart',
        'src/two.dart',
        'src/three.dart',
      ];
    await _pumpExplorer(tester, service);
    await tester.enterText(find.byType(TextField), 'dart');
    await tester.pumpAndSettle();

    await tester.tap(find.text('src/one.dart'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.tap(find.text('src/three.dart'));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();
    await tester.tap(find.text('src/one.dart'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Copy'));
    await tester.pumpAndSettle();

    expect(service.systemClipboardWrites, hasLength(1));
    expect(
      service.systemClipboardWrites.single.paths.map(p.basename).toList(),
      <String>['one.dart', 'three.dart'],
    );
  });

  testWidgets('filtered Ctrl+A and Ctrl+C select visible search results', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..quickOpenEntries = <String>[
        'lib/first.dart',
        'lib/second.dart',
        'lib/third.dart',
      ];
    await _pumpExplorer(tester, service);
    await tester.enterText(find.byType(TextField), 'dart');
    await tester.pumpAndSettle();
    await tester.tap(find.text('lib/first.dart'));
    await _pressExplorerFileShortcut(tester, LogicalKeyboardKey.keyA);
    await _pressExplorerFileShortcut(tester, LogicalKeyboardKey.keyC);

    expect(service.systemClipboardWrites, hasLength(1));
    expect(
      service.systemClipboardWrites.single.paths.map(p.basename).toList(),
      <String>['first.dart', 'second.dart', 'third.dart'],
    );
  });

  testWidgets('filtered copy and paste operate beside the clicked file', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..quickOpenEntries = <String>['src/one.dart', 'dest/two.dart'];
    await _pumpExplorer(tester, service);
    await tester.enterText(find.byType(TextField), 'dart');
    await tester.pumpAndSettle();

    await tester.tap(find.text('src/one.dart'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Copy'));
    await tester.pumpAndSettle();

    await tester.tap(
      find.text('dest/two.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Paste'));
    await tester.pumpAndSettle();

    expect(service.copiedFiles, <String>['src/one.dart->dest']);
    expect(service.quickOpenRefreshRequests, contains('/repo/alera'));
  });

  testWidgets(
    'filtered duplicate uses nested source folder without expansion',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..quickOpenEntries = <String>['src/deep/asset.dart'];
      await _pumpExplorer(tester, service);
      await tester.enterText(find.byType(TextField), 'asset');
      await tester.pumpAndSettle();

      await tester.tap(
        find.text('src/deep/asset.dart'),
        buttons: kSecondaryMouseButton,
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Duplicate'));
      await tester.pumpAndSettle();
      expect(service.copiedFiles, <String>['src/deep/asset.dart->src/deep']);
    },
  );

  testWidgets('filtered rename operates on unloaded full relative path', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..quickOpenEntries = <String>['lib/deep/before.dart'];
    await _pumpExplorer(tester, service);
    await tester.enterText(find.byType(TextField), 'before');
    await tester.pumpAndSettle();

    await tester.tap(
      find.text('lib/deep/before.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Rename'),
      110,
      scrollable: find.byType(Scrollable).last,
    );
    await tester.tap(find.text('Rename'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.descendant(
        of: find.byType(Dialog),
        matching: find.byType(TextField),
      ),
      'after.dart',
    );
    await tester.tap(find.text('Create'));
    await tester.pumpAndSettle();

    expect(service.renamedFiles, <String>['lib/deep/before.dart->after.dart']);
    expect(service.quickOpenRefreshRequests, contains('/repo/alera'));
  });

  testWidgets('filtered delete confirms and targets its unloaded path', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..quickOpenEntries = <String>['src/deep/remove_me.dart'];
    await _pumpExplorer(tester, service);
    await tester.enterText(find.byType(TextField), 'remove_me');
    await tester.pumpAndSettle();
    await tester.tap(
      find.text('src/deep/remove_me.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Delete'),
      115,
      scrollable: find.byType(Scrollable).last,
    );
    await tester.tap(find.text('Delete'));
    await tester.pumpAndSettle();
    expect(find.text('Delete remove_me.dart'), findsOneWidget);
    await tester.tap(find.widgetWithText(FilledButton, 'Delete'));
    await tester.pumpAndSettle();

    expect(service.deletedFiles, <String>['src/deep/remove_me.dart']);
    expect(service.quickOpenRefreshRequests, contains('/repo/alera'));
  });

  testWidgets('filtered menu uses selected external editor for nested file', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..quickOpenEntries = <String>['modules/deep/config.json'];
    final launcher = _FakeExternalEditorLauncher();
    await _pumpExplorer(tester, service, externalEditorLauncher: launcher);
    await tester.enterText(find.byType(TextField), 'config');
    await tester.pumpAndSettle();
    await tester.tap(
      find.text('modules/deep/config.json'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Open in Zed'),
      95,
      scrollable: find.byType(Scrollable).last,
    );
    await tester.tap(find.text('Open in Zed'));
    await tester.pumpAndSettle();
    expect(launcher.fileRequests, hasLength(1));
    expect(
      launcher.fileRequests.single.filePath,
      p.join('/repo/alera', 'modules', 'deep', 'config.json'),
    );
  });

  testWidgets('file context menu offers file history and folders do not', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = [
        _file('readme.md'),
        _directory('src', hasChildrenHint: false),
      ];
    await _pumpExplorer(tester, service);
    await tester.tap(find.text('readme.md'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('View File History'), findsOneWidget);
    await tester.tapAt(const Offset(310, 470));
    await tester.pumpAndSettle();
    await tester.tap(find.text('src'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('View File History'), findsNothing);
  });

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
    await tester.enterText(
      find.descendant(
        of: find.byType(Dialog),
        matching: find.byType(TextField),
      ),
      'root.txt',
    );
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

  testWidgets('Ctrl multi-select and Ctrl+C copies selected files', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('one.txt'),
        _file('two.txt'),
        _file('three.txt'),
      ];
    await _pumpExplorer(tester, service);
    await tester.tap(find.text('one.txt'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.tap(find.text('three.txt'));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();
    await _pressExplorerFileShortcut(tester, LogicalKeyboardKey.keyC);
    expect(service.systemClipboardWrites, hasLength(1));
    expect(
      service.systemClipboardWrites.single.paths.map(p.basename).toList(),
      <String>['one.txt', 'three.txt'],
    );
    expect(
      service.systemClipboardWrites.single.operation,
      WorkspaceFileClipboardOperation.copy,
    );
  });

  testWidgets('Ctrl+A selects all visible files for multi-file copy', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('one.txt'),
        _file('two.txt'),
        _file('three.txt'),
      ];
    await _pumpExplorer(tester, service);
    await tester.tap(find.text('one.txt'));
    await _pressExplorerFileShortcut(tester, LogicalKeyboardKey.keyA);
    await _pressExplorerFileShortcut(tester, LogicalKeyboardKey.keyC);
    expect(service.systemClipboardWrites, hasLength(1));
    expect(
      service.systemClipboardWrites.single.paths.map(p.basename).toList(),
      <String>['one.txt', 'two.txt', 'three.txt'],
    );
  });

  testWidgets('Shift range and Ctrl+X cuts all files', (tester) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('one.txt'),
        _file('two.txt'),
        _file('three.txt'),
      ];
    await _pumpExplorer(tester, service);
    await tester.tap(find.text('one.txt'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.tap(find.text('three.txt'));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pumpAndSettle();
    await _pressExplorerFileShortcut(tester, LogicalKeyboardKey.keyX);
    expect(service.systemClipboardWrites, hasLength(1));
    expect(
      service.systemClipboardWrites.single.paths.map(p.basename).toList(),
      <String>['one.txt', 'two.txt', 'three.txt'],
    );
    expect(
      service.systemClipboardWrites.single.operation,
      WorkspaceFileClipboardOperation.cut,
    );
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

  testWidgets('Ctrl+V pastes external files into the selected directory', (
    tester,
  ) async {
    if (!Platform.isWindows) return;
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('dest', hasChildrenHint: false),
      ]
      ..systemClipboard = const WorkspaceFileClipboardPayload(
        paths: <String>[r'C:\outside\one.txt'],
        operation: WorkspaceFileClipboardOperation.copy,
        sequenceNumber: 42,
      );
    await _pumpExplorer(tester, service);

    await tester.tap(find.text('dest'));
    await tester.pumpAndSettle();
    await _pressExplorerPasteShortcut(tester);

    expect(service.importedClipboardCalls, hasLength(1));
    expect(
      service.importedClipboardCalls.single.targetParentRelativePath,
      'dest',
    );
  });

  testWidgets('Ctrl+V pastes external files beside a selected file', (
    tester,
  ) async {
    if (!Platform.isWindows) return;
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('src', hasChildrenHint: true),
      ]
      ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/readme.md'),
      ]
      ..systemClipboard = const WorkspaceFileClipboardPayload(
        paths: <String>[r'C:\outside\one.txt'],
        operation: WorkspaceFileClipboardOperation.copy,
        sequenceNumber: 42,
      );
    await _pumpExplorer(tester, service);

    await tester.tap(find.text('src'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('readme.md'));
    await tester.pumpAndSettle();
    await _pressExplorerPasteShortcut(tester);

    expect(service.importedClipboardCalls, hasLength(1));
    expect(
      service.importedClipboardCalls.single.targetParentRelativePath,
      'src',
    );
  });

  testWidgets('Ctrl+V pastes external files at root when nothing is selected', (
    tester,
  ) async {
    if (!Platform.isWindows) return;
    final service = _FakeWorkspaceFileService()
      ..systemClipboard = const WorkspaceFileClipboardPayload(
        paths: <String>[r'C:\outside\one.txt'],
        operation: WorkspaceFileClipboardOperation.copy,
        sequenceNumber: 42,
      );
    await _pumpExplorer(tester, service);

    await tester.tapAt(const Offset(250, 220));
    await tester.pump();
    await _pressExplorerPasteShortcut(tester);

    expect(service.importedClipboardCalls, hasLength(1));
    expect(service.importedClipboardCalls.single.targetParentRelativePath, '');
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

  testWidgets('context menu opens files with the system default application', (
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
    await tester.tap(find.text('Open with Default Application'));
    await tester.pumpAndSettle();

    expect(opener.defaultOpenedPaths, <String>[
      p.join('/repo/alera', 'readme.md'),
    ]);
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
    await tester.enterText(
      find.descendant(
        of: find.byType(Dialog),
        matching: find.byType(TextField),
      ),
      'created.dart',
    );
    await tester.tap(find.text('Create'));
    await tester.pump();

    await tester.pumpWidget(const SizedBox.shrink());
    createGate.complete();
    await tester.pumpAndSettle();

    expect(service.createdFiles, <String>['created.dart']);
    expect(tester.takeException(), isNull);
  });
}

Future<void> _pressExplorerFileShortcut(
  WidgetTester tester,
  LogicalKeyboardKey key,
) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  await tester.sendKeyEvent(key);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  await tester.pumpAndSettle();
}

Future<void> _pressExplorerPasteShortcut(WidgetTester tester) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  await tester.sendKeyEvent(LogicalKeyboardKey.keyV);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  await tester.pumpAndSettle();
}
