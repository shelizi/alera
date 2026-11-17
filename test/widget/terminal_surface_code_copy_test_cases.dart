part of 'terminal_surface_test.dart';

void _registerTerminalSurfaceCodeCopyTests() {
  testWidgets('hovering Claude code shows copy control and copies code only', (
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
    try {
      final factory = _FakeTerminalPtySessionFactory();
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: factory,
        shellLaunchesBuilder: _testShellLaunches,
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      await _pumpTerminalSurface(tester, session);
      final pty = factory.sessions.single;
      pty.emitOutput(
        utf8.encode(
          'Claude says:\r\n```typescript\r\nconst answer = 42;\r\n'
          '```\r\nNext message',
        ),
      );
      await _pumpTerminalOutput(tester);
      expect(
        terminalBufferTextForTesting(session),
        contains('const answer = 42;'),
      );

      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(
        location: _cellCenter(tester, const xterm.CellOffset(3, 0)),
      );
      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 2)));
      await tester.pump();
      const copyKey = ValueKey<String>('terminal-copy-code-button');
      expect(find.byKey(copyKey), findsOneWidget);
      pty.writes.clear();

      await tester.tap(find.byKey(copyKey));
      await tester.pump();
      expect(clipboardWrites, <String>['const answer = 42;']);
      expect(pty.writes, isEmpty, reason: 'copy must not send input to Claude');

      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(2, 4)));
      await tester.pump();
      expect(find.byKey(copyKey), findsNothing);
      await mouse.removePointer();
    } finally {
      await tester.pumpWidget(const SizedBox());
      debugDefaultTargetPlatformOverride = null;
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      );
    }
  });

  testWidgets(
    'copies an actual Claude Code ANSI-rendered block without fences',
    (tester) async {
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
      try {
        final factory = _FakeTerminalPtySessionFactory();
        final runtime = XtermTerminalRuntime(
          ptySessionFactory: factory,
          shellLaunchesBuilder: _testShellLaunches,
        );
        addTearDown(runtime.dispose);
        final session = runtime.sessionFor(
          workspace: _workspace(),
          tab: _tab(),
        );
        await _pumpTerminalSurface(tester, session);
        final pty = factory.sessions.single;
        pty.emitOutput(
          utf8.encode(
            '❯ test UI\r\n'
            '• \x1b[34mconst\x1b[0m answer = \x1b[32m42\x1b[0m;\r\n'
            '  console.log(answer);\r\n'
            '\r\n'
            '✦ Cooked for 4s',
          ),
        );
        await _pumpTerminalOutput(tester);
        final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
        await mouse.addPointer(
          location: _cellCenter(tester, const xterm.CellOffset(4, 0)),
        );
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 2)));
        await tester.pump();

        const key = ValueKey<String>('terminal-copy-code-button');
        expect(find.byKey(key), findsOneWidget);
        pty.writes.clear();
        await tester.tap(find.byKey(key));
        await tester.pump();
        expect(clipboardWrites, <String>[
          'const answer = 42;\nconsole.log(answer);',
        ]);
        expect(pty.writes, isEmpty);
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 4)));
        await tester.pump();
        expect(find.byKey(key), findsNothing);
        await mouse.removePointer();
      } finally {
        await tester.pumpWidget(const SizedBox());
        debugDefaultTargetPlatformOverride = null;
        tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        );
      }
    },
  );
}
