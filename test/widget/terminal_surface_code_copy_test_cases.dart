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
        initialSettings: TerminalSettings.defaults.copyWith(
          showCodeBlockCopyButton: true,
        ),
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

      await mouse.moveTo(tester.getCenter(find.byKey(copyKey)));
      await tester.pump();
      await mouse.down(tester.getCenter(find.byKey(copyKey)));
      await mouse.up();
      await tester.pump();
      expect(clipboardWrites, <String>['const answer = 42;']);
      expect(pty.writes, isEmpty, reason: 'copy must not send input to Claude');

      // Clicking must not consume the button: a second copy should work
      // without leaving or re-entering this code block.
      expect(find.byKey(copyKey), findsOneWidget);
      await mouse.moveTo(tester.getCenter(find.byKey(copyKey)));
      await tester.pump();
      await mouse.down(tester.getCenter(find.byKey(copyKey)));
      await mouse.up();
      await tester.pump();
      expect(clipboardWrites, <String>[
        'const answer = 42;',
        'const answer = 42;',
      ]);
      expect(find.byKey(copyKey), findsOneWidget);
      expect(pty.writes, isEmpty);
      // The copied checkmark is transient: restore the copy icon so the
      // control does not look permanently consumed after the first click.
      await tester.pump(const Duration(milliseconds: 1250));
      expect(find.byIcon(Icons.content_copy), findsOneWidget);
      expect(find.byKey(copyKey), findsOneWidget);
      await mouse.down(tester.getCenter(find.byKey(copyKey)));
      await mouse.up();
      await tester.pump();
      expect(clipboardWrites, hasLength(3));
      expect(clipboardWrites.last, 'const answer = 42;');
      expect(find.byKey(copyKey), findsOneWidget);

      // Tooltip/overlay hit-test changes can report MouseRegion exit even
      // though the pointer is still on the copy button. Keep it visible.
      final terminalRegion = tester
          .widgetList<MouseRegion>(
            find.ancestor(
              of: find.byKey(copyKey),
              matching: find.byType(MouseRegion),
            ),
          )
          .lastWhere(
            (region) => region.onHover != null && region.onExit != null,
          );
      terminalRegion.onExit!(
        PointerExitEvent(position: tester.getCenter(find.byKey(copyKey))),
      );
      await tester.pump();
      expect(find.byKey(copyKey), findsOneWidget);

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
          initialSettings: TerminalSettings.defaults.copyWith(
            showCodeBlockCopyButton: true,
          ),
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

  testWidgets('copies only a second Claude snippet without a bullet', (
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
        initialSettings: TerminalSettings.defaults.copyWith(
          showCodeBlockCopyButton: true,
        ),
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      await _pumpTerminalSurface(tester, session);
      final pty = factory.sessions.single;
      pty.emitOutput(
        utf8.encode(
          '❯ UI test\r\n'
          '• const first = 1;\r\n'
          '  console.log(first);\r\n'
          '\r\n'
          '  This is another block.\r\n'
          '\r\n'
          '  const second = 2;\r\n'
          '  console.log(second);\r\n'
          '\r\n'
          '✻ Cooked for 2s',
        ),
      );
      await _pumpTerminalOutput(tester);

      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(
        location: _cellCenter(tester, const xterm.CellOffset(4, 0)),
      );
      const copyKey = ValueKey<String>('terminal-copy-code-button');
      pty.writes.clear();

      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 2)));
      await tester.pump();
      expect(find.byKey(copyKey), findsOneWidget);
      await tester.tap(find.byKey(copyKey));
      await tester.pump();
      expect(clipboardWrites, <String>[
        'const first = 1;\nconsole.log(first);',
      ]);

      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 4)));
      await tester.pump();
      expect(find.byKey(copyKey), findsNothing);

      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 7)));
      await tester.pump();
      expect(find.byKey(copyKey), findsOneWidget);
      await tester.tap(find.byKey(copyKey));
      await tester.pump();
      expect(clipboardWrites, <String>[
        'const first = 1;\nconsole.log(first);',
        'const second = 2;\nconsole.log(second);',
      ]);
      expect(pty.writes, isEmpty);

      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 4)));
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
    'Claude titled JavaScript and C snippets get separate copy buttons',
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
          initialSettings: TerminalSettings.defaults.copyWith(
            showCodeBlockCopyButton: true,
          ),
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
            '❯ 給我常見語言的程式碼\r\n'
            '• 以下是一些 Hello World 範例:\r\n'
            '\r\n'
            'Python\r\n'
            'print("Hello, World!")\r\n'
            '\r\n'
            'JavaScript (Node.js)\r\n'
            'console.log("Hello, World!");\r\n'
            '\r\n'
            'C\r\n'
            '#include <stdio.h>\r\n'
            '\r\n'
            'int main(void) {\r\n'
            '  printf("Hello, World!");\r\n'
            '  return 0;\r\n'
            '}\r\n'
            '\r\n'
            '✻ Brewed for 4s',
          ),
        );
        await _pumpTerminalOutput(tester);
        final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
        await mouse.addPointer(
          location: _cellCenter(tester, const xterm.CellOffset(3, 0)),
        );
        const copyKey = ValueKey<String>('terminal-copy-code-button');
        pty.writes.clear();

        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 7)));
        await tester.pump();
        expect(find.byKey(copyKey), findsOneWidget);
        await tester.tap(find.byKey(copyKey));
        await tester.pump();
        expect(clipboardWrites, <String>['console.log("Hello, World!");']);

        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 13)));
        await tester.pump();
        expect(find.byKey(copyKey), findsOneWidget);
        await tester.tap(find.byKey(copyKey));
        await tester.pump();
        expect(clipboardWrites, <String>[
          'console.log("Hello, World!");',
          '#include <stdio.h>\n\nint main(void) {\n  printf("Hello, World!");\n  return 0;\n}',
        ]);
        expect(pty.writes, isEmpty, reason: 'copy must not modify Claude PTY');

        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 9)));
        await tester.pump();
        expect(
          find.byKey(copyKey),
          findsNothing,
          reason: 'C language heading is not itself source code',
        );
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

  testWidgets('copies full unlabelled Claude eight-queens C source by mouse', (
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
        initialSettings: TerminalSettings.defaults.copyWith(
          showCodeBlockCopyButton: true,
        ),
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      await _pumpTerminalSurface(tester, session);
      final pty = factory.sessions.single;
      const code =
          '#include <stdio.h>\n'
          '\n'
          '#define N 8\n'
          '\n'
          'static int queens[N];\n'
          '\n'
          'static void solve(int row) {\n'
          '  if (row == N) {\n'
          '    printf("Solved");\n'
          '    return;\n'
          '  }\n'
          '}';
      pty.emitOutput(
        utf8.encode(
          '❯ 給我八皇后程式碼\r\n'
          '• 下面是八皇后問題的 C 實作，用回溯法找出 92 組解。\r\n'
          '\r\n'
          '${code.replaceAll('\n', '\r\n')}\r\n'
          '\r\n'
          '運作方式\r\n'
          '- 每列放一個皇后。\r\n'
          '✻ Crunched for 8s',
        ),
      );
      await _pumpTerminalOutput(tester);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(
        location: _cellCenter(tester, const xterm.CellOffset(4, 0)),
      );
      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 9)));
      await tester.pump();
      const buttonKey = ValueKey<String>('terminal-copy-code-button');
      expect(find.byKey(buttonKey), findsOneWidget);

      pty.writes.clear();
      await tester.tap(find.byKey(buttonKey));
      await tester.pump();
      expect(clipboardWrites, <String>[code]);
      expect(pty.writes, isEmpty);

      // The same source is still copyable after clicking.
      expect(find.byKey(buttonKey), findsOneWidget);
      await tester.tap(find.byKey(buttonKey));
      await tester.pump();
      expect(clipboardWrites, <String>[code, code]);

      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 16)));
      await tester.pump();
      expect(
        find.byKey(buttonKey),
        findsNothing,
        reason: 'Claude explanation is not source code',
      );
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
    'copy button defaults off and toggles live without restarting the PTY',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.windows;
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
            'Claude says:\r\n'
            '~~~dart\r\n'
            'void main() {}\r\n'
            '~~~\r\n'
            'Done',
          ),
        );
        await _pumpTerminalOutput(tester);
        final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
        await mouse.addPointer(
          location: _cellCenter(tester, const xterm.CellOffset(3, 0)),
        );
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 2)));
        await tester.pump();

        const copyKey = ValueKey<String>('terminal-copy-code-button');
        expect(runtime.terminalSettings.showCodeBlockCopyButton, isFalse);
        expect(find.byKey(copyKey), findsNothing);

        runtime.updateSettings(
          TerminalSettings.defaults.copyWith(showCodeBlockCopyButton: true),
        );
        await tester.pump();
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 1)));
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 2)));
        await tester.pump();
        expect(find.byKey(copyKey), findsOneWidget);

        runtime.updateSettings(TerminalSettings.defaults);
        await tester.pump();
        expect(find.byKey(copyKey), findsNothing);
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 1)));
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 2)));
        await tester.pump();
        expect(find.byKey(copyKey), findsNothing);

        runtime.updateSettings(
          TerminalSettings.defaults.copyWith(showCodeBlockCopyButton: true),
        );
        await tester.pump();
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 1)));
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 2)));
        await tester.pump();
        expect(find.byKey(copyKey), findsOneWidget);
        expect(
          pty.writes,
          isEmpty,
          reason: 'setting changes must not send PTY input',
        );
        await mouse.removePointer();
      } finally {
        await tester.pumpWidget(const SizedBox());
        debugDefaultTargetPlatformOverride = null;
      }
    },
  );
}
