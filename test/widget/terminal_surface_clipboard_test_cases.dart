part of 'terminal_surface_test.dart';

void _registerTerminalSurfaceClipboardTests() {
  testWidgets('Claude mouse tracking allows shift selection and copy', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.windows;
    final clipboardWrites = <String>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          clipboardWrites.add(
            (call.arguments as Map<Object?, Object?>)['text'] as String,
          );
        }
        return null;
      },
    );
    addTearDown(() {
      debugDefaultTargetPlatformOverride = null;
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      );
    });

    final factory = _FakeTerminalPtySessionFactory();
    final runtime = XtermTerminalRuntime(
      ptySessionFactory: factory,
      shellLaunchesBuilder: _testShellLaunches,
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    await _pumpTerminalSurface(tester, session);
    final pty = factory.sessions.single;
    pty.emitOutput(utf8.encode('copy this text\r\n\x1b[?1003h\x1b[?1006h'));
    await _pumpTerminalOutput(tester);
    expect(
      terminalMouseModeForTesting(session),
      xterm.MouseMode.upDownScrollMove,
    );
    pty.writes.clear();

    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.down(_cellCenter(tester, const xterm.CellOffset(0, 0)));
    await tester.pump(kPressTimeout);
    await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(9, 0)));
    await tester.pump();
    await mouse.up();
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pump();

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyC);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();

    expect(clipboardWrites, contains('copy this'));
    expect(
      pty.writes,
      isEmpty,
      reason: 'select and copy must not forward mouse reports or Ctrl+C',
    );
    // Flutter verifies foundation globals before addTearDown callbacks run.
    // Restore the test-only platform override within the test body.
    await tester.pumpWidget(const SizedBox());
    debugDefaultTargetPlatformOverride = null;
  });
  testWidgets('focused terminal denies clipboard reads and gates writes', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    try {
      final clipboardCalls = <MethodCall>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.getData' ||
              call.method == 'Clipboard.setData') {
            clipboardCalls.add(call);
          }
          if (call.method == 'Clipboard.getData') return {'text': 'private'};
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        ),
      );
      final factory = _FakeTerminalPtySessionFactory();
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: factory,
        shellLaunchesBuilder: _testShellLaunches,
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      await _pumpTerminalSurface(tester, session);
      final view = tester.widget<xterm.TerminalView>(
        find.byType(xterm.TerminalView),
      );
      view.focusNode!.requestFocus();
      await tester.pump();
      expect(view.focusNode!.hasFocus, isTrue);
      final pty = factory.sessions.single;
      pty.writes.clear();
      final payload = base64.encode(utf8.encode('allowed write'));

      pty.emitOutput(utf8.encode('\x1b]52;c;?\x07\x1b]52;c;$payload\x07'));
      await _pumpTerminalOutput(tester);
      expect(clipboardCalls, isEmpty);
      expect(pty.writes, isEmpty);

      runtime.updateSettings(
        TerminalSettings.defaults.copyWith(allowOsc52Clipboard: true),
      );
      pty.emitOutput(utf8.encode('\x1b]52;c;?\x07\x1b]52;c;$payload\x07'));
      await _pumpTerminalOutput(tester);
      expect(clipboardCalls.map((call) => call.method), ['Clipboard.setData']);
      expect(clipboardCalls.single.arguments, {'text': 'allowed write'});
      expect(pty.writes, isEmpty);
      await tester.pumpWidget(const SizedBox());
    } finally {
      debugDefaultTargetPlatformOverride = null;
    }
  });
}
