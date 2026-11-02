part of 'workspace_explorer_test.dart';

void _registerWorkspaceExplorerExternalDropTests() {
  testWidgets('OS files and directories dropped onto a folder are copied', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('destination', hasChildrenHint: false),
      ];
    await _pumpExplorer(tester, service);

    final target = tester.widget<DropTarget>(find.byType(DropTarget));
    target.onDragDone!(
      DropDoneDetails(
        files: <DropItem>[
          DropItemFile('/outside/example.txt'),
          DropItemDirectory('/outside/photos', <DropItem>[]),
        ],
        localPosition: Offset.zero,
        globalPosition: tester.getCenter(find.text('destination')),
      ),
    );
    await tester.pumpAndSettle();

    expect(service.importedClipboardCalls, hasLength(1));
    final imported = service.importedClipboardCalls.single;
    expect(imported.sourcePaths, <String>[
      '/outside/example.txt',
      '/outside/photos',
    ]);
    expect(imported.targetParentRelativePath, 'destination');
    expect(imported.moveSources, isFalse);
  });

  testWidgets('OS drop on blank Explorer space imports to workspace root', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('first.txt'),
      ];
    await _pumpExplorer(tester, service);
    final target = tester.widget<DropTarget>(find.byType(DropTarget));
    final bounds = tester.getRect(find.byType(DropTarget));
    target.onDragDone!(
      DropDoneDetails(
        files: <DropItem>[DropItemFile('/outside/second.txt')],
        localPosition: Offset.zero,
        globalPosition: bounds.bottomRight - const Offset(16, 16),
      ),
    );
    await tester.pumpAndSettle();

    expect(service.importedClipboardCalls, hasLength(1));
    expect(
      service.importedClipboardCalls.single.targetParentRelativePath,
      isEmpty,
    );
  });

  testWidgets('OS drop on filtered file row imports into its parent folder', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..quickOpenEntries = <String>['src/deep/first.dart'];
    await _pumpExplorer(tester, service);
    await tester.enterText(find.byType(TextField), 'first.dart');
    await tester.pumpAndSettle();

    final target = tester.widget<DropTarget>(find.byType(DropTarget));
    target.onDragDone!(
      DropDoneDetails(
        files: <DropItem>[DropItemFile('/outside/second.dart')],
        localPosition: Offset.zero,
        globalPosition: tester.getCenter(find.text('src/deep/first.dart')),
      ),
    );
    await tester.pumpAndSettle();

    expect(service.importedClipboardCalls, hasLength(1));
    expect(
      service.importedClipboardCalls.single.targetParentRelativePath,
      'src/deep',
    );
  });
}

void _registerWorkspaceExplorerNativeDragTests() {
  testWidgets('filtered Windows drag copies selected files to Explorer', (
    tester,
  ) async {
    if (!Platform.isWindows) return;

    const channel = MethodChannel('dev.leynier.alera/native_file_drag');
    final nativeCalls = <MethodCall>[];
    final messenger = tester.binding.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(channel, (call) async {
      nativeCalls.add(call);
      return switch (call.method) {
        'cursorOutsideWindow' => false,
        'startDrag' => 'copy',
        _ => throw MissingPluginException(),
      };
    });
    addTearDown(() => messenger.setMockMethodCallHandler(channel, null));

    final service = _FakeWorkspaceFileService()
      ..quickOpenEntries = <String>[
        'src/deep/first.dart',
        'src/deep/second.dart',
        'src/deep/third.dart',
      ];
    await _pumpExplorer(tester, service);
    await tester.enterText(find.byType(TextField), 'dart');
    await tester.pumpAndSettle();
    await tester.tap(find.text('src/deep/first.dart'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.tap(find.text('src/deep/third.dart'));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();

    final drag = await tester.startGesture(
      tester.getCenter(find.text('src/deep/first.dart')),
      kind: PointerDeviceKind.mouse,
    );
    await drag.moveBy(const Offset(50, 0));
    await tester.pump();
    await drag.moveBy(const Offset(950, 0));
    await tester.pumpAndSettle();
    await drag.up();
    await tester.pumpAndSettle();

    final nativeDrag = nativeCalls.where((call) => call.method == 'startDrag');
    expect(nativeDrag, hasLength(1));
    final paths =
        ((nativeDrag.single.arguments as Map<Object?, Object?>)['paths']
                as List<Object?>)
            .cast<String>();
    expect(paths.map(p.basename).toList(), <String>[
      'first.dart',
      'third.dart',
    ]);
  });

  testWidgets('filtered folder drag keeps folder as native Shell item', (
    tester,
  ) async {
    if (!Platform.isWindows) return;
    const channel = MethodChannel('dev.leynier.alera/native_file_drag');
    final nativeCalls = <MethodCall>[];
    final messenger = tester.binding.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(channel, (call) async {
      nativeCalls.add(call);
      return switch (call.method) {
        'cursorOutsideWindow' => false,
        'startDrag' => 'copy',
        _ => throw MissingPluginException(),
      };
    });
    addTearDown(() => messenger.setMockMethodCallHandler(channel, null));

    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('src', hasChildrenHint: true),
      ]
      ..quickOpenEntries = <String>['src/deep/first.dart'];
    await _pumpExplorer(tester, service);
    await tester.enterText(find.byType(TextField), 'src');
    await tester.pumpAndSettle();
    expect(find.text('src'), findsNWidgets(2));
    expect(find.text('src/deep/first.dart'), findsOneWidget);

    // Select folder plus its descendant: the shared payload contains only
    // the top-level folder, just as the normal tree does.
    await tester.tap(find.text('src').last, buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.tap(find.text('src/deep/first.dart'));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();

    final drag = await tester.startGesture(
      tester.getCenter(find.text('src').last),
      kind: PointerDeviceKind.mouse,
    );
    await drag.moveBy(const Offset(50, 0));
    await tester.pump();
    await drag.moveBy(const Offset(950, 0));
    await tester.pumpAndSettle();
    await drag.up();
    await tester.pumpAndSettle();

    final nativeDrag = nativeCalls.where((call) => call.method == 'startDrag');
    expect(nativeDrag, hasLength(1));
    final paths =
        ((nativeDrag.single.arguments as Map<Object?, Object?>)['paths']
                as List<Object?>)
            .cast<String>();
    expect(paths.map(p.basename), <String>['src']);
  });

  testWidgets('Windows drag leaving Flutter hands selected files to Shell', (
    tester,
  ) async {
    if (!Platform.isWindows) return;

    const channel = MethodChannel('dev.leynier.alera/native_file_drag');
    final nativeCalls = <MethodCall>[];
    final messenger = tester.binding.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(channel, (call) async {
      nativeCalls.add(call);
      return switch (call.method) {
        'cursorOutsideWindow' => false,
        'startDrag' => 'copy',
        _ => throw MissingPluginException(),
      };
    });
    addTearDown(() => messenger.setMockMethodCallHandler(channel, null));

    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('first.txt'),
        _file('second.txt'),
      ];
    await _pumpExplorer(tester, service);
    await tester.tap(find.text('first.txt'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.tap(find.text('second.txt'));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();

    final gesture = await tester.startGesture(
      tester.getCenter(find.text('first.txt')),
      kind: PointerDeviceKind.mouse,
    );
    await gesture.moveBy(const Offset(50, 0));
    await tester.pump();
    await gesture.moveBy(const Offset(950, 0));
    await tester.pumpAndSettle();
    await gesture.up();
    await tester.pumpAndSettle();

    final dragCalls = nativeCalls
        .where((call) => call.method == 'startDrag')
        .toList();
    expect(dragCalls, hasLength(1));
    final args = dragCalls.single.arguments as Map<Object?, Object?>;
    final paths = (args['paths'] as List<Object?>).cast<String>();
    expect(
      paths.map(p.basename),
      containsAll(<String>['first.txt', 'second.txt']),
    );
  });
}
