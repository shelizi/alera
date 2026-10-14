part of 'workspace_explorer_test.dart';

void _registerWorkspaceExplorerNativeDragTests() {
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
