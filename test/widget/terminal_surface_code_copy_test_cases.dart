part of 'terminal_surface_test.dart';

void _registerTerminalSurfaceCodeCopyTests() {
  testWidgets(
    'manual Claude transcript makes two source copy buttons available',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.windows;
      final copied = <String>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.setData') {
            copied.add(
              (call.arguments as Map<Object?, Object?>)['text'] as String,
            );
          }
          return null;
        },
      );
      try {
        final sourceReady = Completer<List<ClaudeSourceCodeBlock>>();
        var reads = 0;
        final store = ClaudeCodeMessageStore();
        addTearDown(store.dispose);
        final factory = _FakeTerminalPtySessionFactory();
        final runtime = XtermTerminalRuntime(
          ptySessionFactory: factory,
          shellLaunchesBuilder: _testShellLaunches,
          claudeCodeMessageStore: store,
          claudeTranscriptSourceLoader: (path) {
            reads++;
            expect(path, '/tmp/manual-claude-repo');
            return sourceReady.future;
          },
          initialSettings: TerminalSettings.defaults.copyWith(
            showCodeBlockCopyButton: true,
          ),
        );
        addTearDown(runtime.dispose);
        final session = runtime.sessionFor(
          workspace: _workspace(path: '/tmp/manual-claude-repo'),
          tab: _tab(),
        );
        await _pumpTerminalSurface(tester, session);
        factory.sessions.single.emitOutput(
          utf8.encode(
            '❯ manual claude\r\n'
            '• let answer = 42\r\n'
            '  print(answer)\r\n'
            '\r\n'
            'Objective-C\r\n'
            '#import <Foundation/Foundation.h>\r\n'
            '\r\n'
            'int main(void) { return 0; }\r\n'
            '\r\n'
            'Plain explanation, not code',
          ),
        );
        await _pumpTerminalOutput(tester);
        final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
        await mouse.addPointer(
          location: _cellCenter(tester, const xterm.CellOffset(3, 0)),
        );
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 1)));
        await tester.pump();
        const key = ValueKey<String>('terminal-copy-code-button');
        expect(
          find.byKey(key),
          findsNothing,
          reason: 'Never show speculative buttons without a canonical source',
        );
        expect(reads, 1);
        sourceReady.complete(const [
          ClaudeSourceCodeBlock(
            messageId: 'm1',
            code: 'let answer = 42\nprint(answer)',
          ),
          ClaudeSourceCodeBlock(
            messageId: 'm1',
            code:
                '#import <Foundation/Foundation.h>\n'
                'int main(void) { return 0; }',
          ),
        ]);
        await tester.pump();
        await tester.pump();
        expect(
          find.byKey(key),
          findsOneWidget,
          reason: 'Original transcript arriving under cursor shows the button',
        );
        expect(
          find.byWidgetPredicate((candidate) {
            if (candidate is! IconButton) return false;
            final key = candidate.key;
            return key is ValueKey<String> &&
                key.value.startsWith('terminal-copy-code-button');
          }),
          findsNWidgets(2),
          reason: 'Both visible code blocks must expose copy actions at once',
        );

        await tester.tap(find.byKey(key));
        await tester.pump();
        expect(copied, ['let answer = 42\nprint(answer)']);
        await tester.tap(find.byKey(key));
        await tester.pump();
        expect(copied, [
          'let answer = 42\nprint(answer)',
          'let answer = 42\nprint(answer)',
        ]);

        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 7)));
        await tester.pump();
        expect(find.byKey(key), findsOneWidget);
        await tester.tap(find.byKey(key));
        await tester.pump();
        expect(
          copied.last,
          '#import <Foundation/Foundation.h>\nint main(void) { return 0; }',
        );
        expect(
          reads,
          1,
          reason: 'Hovering between blocks must not re-read JSONL',
        );
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 9)));
        await tester.pump();
        expect(
          find.byKey(key),
          findsOneWidget,
          reason:
              'Copy actions stay visible when the mouse is over non-code text',
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

  testWidgets('80-line Claude code stays copyable after viewport scrollback', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.windows;
    final copied = <String>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copied.add(
            (call.arguments as Map<Object?, Object?>)['text'] as String,
          );
        }
        return null;
      },
    );
    try {
      final store = ClaudeCodeMessageStore();
      addTearDown(store.dispose);
      final factory = _FakeTerminalPtySessionFactory();
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: factory,
        shellLaunchesBuilder: _testShellLaunches,
        claudeCodeMessageStore: store,
        initialSettings: TerminalSettings.defaults.copyWith(
          showCodeBlockCopyButton: true,
        ),
      );
      addTearDown(runtime.dispose);
      final tab = _tab();
      final session = runtime.sessionFor(workspace: _workspace(), tab: tab);
      await _pumpTerminalSurface(tester, session);
      final lines = List<String>.generate(
        80,
        (i) =>
            'line_' +
            (i + 1).toString().padLeft(3, '0') +
            ' = "ALERA_LONG_' +
            (i + 1).toString().padLeft(3, '0') +
            '"',
      );
      final code = lines.join('\n');
      factory.sessions.single.emitOutput(
        utf8.encode(
          [
            '❯ request 80 source lines',
            '• ' + lines.first,
            ...lines.skip(1),
            'Completed.',
          ].join('\r\n'),
        ),
      );
      await _pumpTerminalOutput(tester);
      store.observeHookEvent(
        AgentHookEvent(
          terminalSessionId: tab.terminalSessionId,
          workspaceId: tab.workspaceId,
          tabId: tab.id,
          agentType: AgentType.claude,
          hookEventName: 'Stop',
          payload: <String, Object?>{
            'session_id': 'claude-long-scroll',
            'transcript_path': 'claude-long-scroll.jsonl',
            'last_assistant_message':
                '\u0060\u0060\u0060python\n' + code + '\n\u0060\u0060\u0060',
          },
        ),
      );

      final terminal = tester
          .widget<xterm.TerminalView>(find.byType(xterm.TerminalView))
          .terminal;
      expect(terminal.buffer.scrollBack, greaterThan(0));
      int locate(String pattern) {
        for (var i = 0; i < terminal.buffer.lines.length; i++) {
          if (terminal.buffer.lines[i].getText().contains(pattern)) {
            return i;
          }
        }
        throw StateError('Cannot locate line: ' + pattern);
      }

      final row65 = locate('ALERA_LONG_065');
      final row78 = locate('ALERA_LONG_078');
      final lastRow = locate('ALERA_LONG_080');
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(
        location: _cellCenter(tester, xterm.CellOffset(4, row65 - 1)),
      );
      await mouse.moveTo(_cellCenter(tester, xterm.CellOffset(4, row65)));
      await tester.pump();
      const copyKey = ValueKey<String>('terminal-copy-code-button');
      expect(
        find.byKey(copyKey),
        findsOneWidget,
        reason: 'Copy action must exist when source start is offscreen',
      );

      final render = tester
          .state<xterm.TerminalViewState>(find.byType(xterm.TerminalView))
          .renderTerminal;
      final afterLastLine = render
          .localToGlobal(
            render.getOffset(xterm.CellOffset(0, lastRow)) +
                Offset(0, render.cellSize.height),
          )
          .dy;
      final initialButtonTop = tester.getTopLeft(find.byKey(copyKey)).dy;
      final viewportBottom = tester
          .getBottomLeft(find.byType(xterm.TerminalView))
          .dy;
      final expectedTop = (afterLastLine + 4).clamp(4.0, viewportBottom - 36);
      expect(
        initialButtonTop,
        closeTo(expectedTop, 1),
        reason:
            'Anchor is below code when space permits, otherwise pinned '
            'to the reachable viewport bottom edge',
      );

      await mouse.moveTo(_cellCenter(tester, xterm.CellOffset(4, row78)));
      await tester.pump();
      expect(
        tester.getTopLeft(find.byKey(copyKey)).dy,
        closeTo(initialButtonTop, 1),
        reason: 'Hovering later lines must not move the copy button',
      );

      await tester.tap(find.byKey(copyKey));
      await tester.pump();
      expect(copied, [code]);
      await tester.tap(find.byKey(copyKey));
      await tester.pump();
      expect(copied, [code, code]);
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

  testWidgets('long single canonical line survives TUI visual soft wraps', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.windows;
    final copied = <String>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copied.add(
            (call.arguments as Map<Object?, Object?>)['text'] as String,
          );
        }
        return null;
      },
    );
    try {
      final store = ClaudeCodeMessageStore();
      addTearDown(store.dispose);
      final factory = _FakeTerminalPtySessionFactory();
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: factory,
        shellLaunchesBuilder: _testShellLaunches,
        claudeCodeMessageStore: store,
        initialSettings: TerminalSettings.defaults.copyWith(
          showCodeBlockCopyButton: true,
        ),
      );
      addTearDown(runtime.dispose);
      final tab = _tab();
      final session = runtime.sessionFor(workspace: _workspace(), tab: tab);
      await _pumpTerminalSurface(tester, session);
      final code =
          'const wrapped = "ALERA_WRAP_TEST_' +
          List.filled(20, '0123456789abcdef').join() +
          '";';
      final chunks = <String>[];
      for (var index = 0; index < code.length; index += 55) {
        final end = (index + 55).clamp(0, code.length);
        chunks.add(code.substring(index, end));
      }
      factory.sessions.single.emitOutput(
        utf8.encode(
          [
            '❯ create long source',
            '• ' + chunks.first,
            for (final part in chunks.skip(1)) '  ' + part,
            '',
            'Explanation follows',
          ].join('\r\n'),
        ),
      );
      await _pumpTerminalOutput(tester);
      store.observeHookEvent(
        AgentHookEvent(
          terminalSessionId: tab.terminalSessionId,
          workspaceId: tab.workspaceId,
          tabId: tab.id,
          agentType: AgentType.claude,
          hookEventName: 'Stop',
          payload: <String, Object?>{
            'session_id': 'claude-soft-wrapped',
            'transcript_path': 'claude-soft-wrapped.jsonl',
            'last_assistant_message':
                '\u0060\u0060\u0060typescript\n' +
                code +
                '\n\u0060\u0060\u0060',
          },
        ),
      );
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(
        location: _cellCenter(tester, const xterm.CellOffset(5, 0)),
      );
      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(5, 3)));
      await tester.pump();
      const key = ValueKey<String>('terminal-copy-code-button');
      expect(
        find.byKey(key),
        findsOneWidget,
        reason: 'Hovering a middle TUI continuation row must expose action',
      );

      final render = tester
          .state<xterm.TerminalViewState>(find.byType(xterm.TerminalView))
          .renderTerminal;
      final lastCodeBottom = render
          .localToGlobal(
            render.getOffset(xterm.CellOffset(0, chunks.length)) +
                Offset(0, render.cellSize.height),
          )
          .dy;
      final maxTop =
          tester.getBottomLeft(find.byType(xterm.TerminalView)).dy - 36;
      final expected = (lastCodeBottom + 4).clamp(4.0, maxTop);
      expect(tester.getTopLeft(find.byKey(key)).dy, closeTo(expected, 1));

      await tester.tap(find.byKey(key));
      await tester.pump();
      expect(copied, <String>[code]);
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

  testWidgets('copy button is fixed below the last rendered code row', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.windows;
    final copies = <String>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copies.add(
            (call.arguments as Map<Object?, Object?>)['text'] as String,
          );
        }
        return null;
      },
    );
    try {
      final store = ClaudeCodeMessageStore();
      addTearDown(store.dispose);
      final factory = _FakeTerminalPtySessionFactory();
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: factory,
        shellLaunchesBuilder: _testShellLaunches,
        claudeCodeMessageStore: store,
        initialSettings: TerminalSettings.defaults.copyWith(
          showCodeBlockCopyButton: true,
        ),
      );
      addTearDown(runtime.dispose);
      final tab = _tab();
      final session = runtime.sessionFor(workspace: _workspace(), tab: tab);
      await _pumpTerminalSurface(tester, session);
      final codeLines = List<String>.generate(
        18,
        (index) =>
            'item_' +
            (index + 1).toString() +
            ' = ' +
            (index + 1).toString() +
            ';',
      );
      final code = codeLines.join('\n');
      final renderedLines = <String>[
        '❯ show long code',
        '• ' + codeLines.first,
        for (final line in codeLines.skip(1)) '  ' + line,
        '',
        'Explanation follows the block.',
      ];
      factory.sessions.single.emitOutput(
        utf8.encode(renderedLines.join('\r\n')),
      );
      await _pumpTerminalOutput(tester);
      store.observeHookEvent(
        AgentHookEvent(
          terminalSessionId: tab.terminalSessionId,
          workspaceId: tab.workspaceId,
          tabId: tab.id,
          agentType: AgentType.claude,
          hookEventName: 'Stop',
          payload: <String, Object?>{
            'session_id': 'claude-layout-test',
            'transcript_path': 'claude-layout-test.jsonl',
            'last_assistant_message':
                '\u0060\u0060\u0060text\n' + code + '\n\u0060\u0060\u0060',
          },
        ),
      );

      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(
        location: _cellCenter(tester, const xterm.CellOffset(5, 0)),
      );
      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(5, 2)));
      await tester.pump();
      const copyKey = ValueKey<String>('terminal-copy-code-button');
      expect(find.byKey(copyKey), findsOneWidget);

      final renderer = tester
          .state<xterm.TerminalViewState>(find.byType(xterm.TerminalView))
          .renderTerminal;
      const lastCodeRow = 18;
      final lastCodeBottom = renderer.localToGlobal(
        renderer.getOffset(const xterm.CellOffset(0, lastCodeRow)) +
            Offset(0, renderer.cellSize.height),
      );
      final topFromFirstHover = tester.getTopLeft(find.byKey(copyKey)).dy;
      expect(topFromFirstHover, closeTo(lastCodeBottom.dy + 4, 1));

      // Moving over another row in the same code block must not move the UI.
      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(5, 12)));
      await tester.pump();
      expect(
        tester.getTopLeft(find.byKey(copyKey)).dy,
        closeTo(topFromFirstHover, 1),
      );
      expect(
        tester.getTopLeft(find.byKey(copyKey)).dy,
        greaterThan(lastCodeBottom.dy),
      );

      await tester.tap(find.byKey(copyKey));
      await tester.pump();
      expect(copies, <String>[code]);
      await tester.tap(find.byKey(copyKey));
      await tester.pump();
      expect(copies, <String>[code, code]);
      expect(find.byKey(copyKey), findsOneWidget);
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

  testWidgets('Claude agent launch adds copy Hook settings only when enabled', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.windows;
    try {
      final launches = <({bool enabled, String command})>[];
      for (final enabled in <bool>[false, true]) {
        final factory = _FakeTerminalPtySessionFactory();
        final runtime = XtermTerminalRuntime(
          ptySessionFactory: factory,
          shellLaunchesBuilder: _testShellLaunches,
          agentHookEnvironmentBuilder:
              ({
                required terminalSessionId,
                required workspaceId,
                required tabId,
              }) => <String, String>{
                'ALERA_CLAUDE_COPY_SETTINGS':
                    r'C:\Program Files\Alera\claude\settings.json',
              },
          initialSettings: TerminalSettings.defaults.copyWith(
            showCodeBlockCopyButton: enabled,
          ),
        );
        addTearDown(runtime.dispose);
        final tab = WorkspaceTabRecord(
          id: enabled ? 'tab-copy-on' : 'tab-copy-off',
          workspaceId: 'ws-1',
          title: 'Claude',
          createdAt: DateTime.utc(2026),
          updatedAt: DateTime.utc(2026),
          payload: {
            'agentType': 'claude',
            workspaceTabInitialCommandPayloadKey: 'claude',
          },
        );
        final session = runtime.sessionFor(workspace: _workspace(), tab: tab);
        await _pumpTerminalSurface(tester, session);
        await tester.pump(const Duration(milliseconds: 600));
        await tester.pump();
        final command = factory.sessions.single.writes.map(utf8.decode).join();
        launches.add((enabled: enabled, command: command));
        await tester.pumpWidget(const SizedBox());
      }
      expect(launches[0].command, contains('claude'));
      expect(launches[0].command, isNot(contains('--settings')));
      expect(launches[1].command, contains('claude --settings'));
      expect(launches[1].command, contains('Program Files'));
    } finally {
      await tester.pumpWidget(const SizedBox());
      debugDefaultTargetPlatformOverride = null;
    }
  });

  testWidgets(
    'Claude Hook copy uses original source beyond rendered viewport',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.windows;
      final copied = <String>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.setData') {
            copied.add(
              (call.arguments as Map<Object?, Object?>)['text'] as String,
            );
          }
          return null;
        },
      );
      try {
        final store = ClaudeCodeMessageStore();
        addTearDown(store.dispose);
        final factory = _FakeTerminalPtySessionFactory();
        final runtime = XtermTerminalRuntime(
          ptySessionFactory: factory,
          shellLaunchesBuilder: _testShellLaunches,
          claudeCodeMessageStore: store,
          initialSettings: TerminalSettings.defaults.copyWith(
            showCodeBlockCopyButton: true,
          ),
        );
        addTearDown(runtime.dispose);
        final tab = _tab();
        final session = runtime.sessionFor(workspace: _workspace(), tab: tab);
        await _pumpTerminalSurface(tester, session);
        final pty = factory.sessions.single;
        pty.emitOutput(
          utf8.encode(
            '❯ generate code\r\n'
            '• value_001 = 1\r\n'
            '  value_002 = 2\r\n'
            '  value_003 = 3\r\n'
            '\r\n'
            'Jump to bottom\r\n',
          ),
        );
        await _pumpTerminalOutput(tester);

        final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
        await mouse.addPointer(
          location: _cellCenter(tester, const xterm.CellOffset(2, 0)),
        );
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 2)));
        await tester.pump();
        const copyKey = ValueKey<String>('terminal-copy-code-button');
        expect(
          find.byKey(copyKey),
          findsNothing,
          reason: 'No language or output heuristics without source',
        );

        final original = List.generate(70, (index) {
          final number = index + 1;
          return 'value_' +
              number.toString().padLeft(3, '0') +
              ' = ' +
              number.toString();
        }).join('\n');
        store.observeHookEvent(
          AgentHookEvent(
            terminalSessionId: tab.terminalSessionId,
            workspaceId: tab.workspaceId,
            tabId: tab.id,
            agentType: AgentType.claude,
            hookEventName: 'MessageDisplay',
            payload: {
              'session_id': 'claude-test-1',
              'transcript_path': 'claude-test-1.jsonl',
              'message_id': 'message-1',
              'index': 0,
              'final': true,
              'delta':
                  '\u0060\u0060\u0060python\n' +
                  original +
                  '\n\u0060\u0060\u0060',
            },
          ),
        );
        await tester.pump();
        expect(
          find.byKey(copyKey),
          findsOneWidget,
          reason: 'Hook update makes button available without mouse reentry',
        );

        pty.writes.clear();
        await tester.tap(find.byKey(copyKey));
        await tester.pump();
        expect(copied, <String>[original]);
        expect(pty.writes, isEmpty);
        expect(find.byKey(copyKey), findsOneWidget);

        await tester.tap(find.byKey(copyKey));
        await tester.pump();
        expect(copied, <String>[original, original]);
        expect(find.byKey(copyKey), findsOneWidget);
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
      final terminalRender = tester
          .state<xterm.TerminalViewState>(find.byType(xterm.TerminalView))
          .renderTerminal;
      final fenceBottom = terminalRender.localToGlobal(
        terminalRender.getOffset(const xterm.CellOffset(0, 3)) +
            Offset(0, terminalRender.cellSize.height),
      );
      expect(
        tester.getTopLeft(find.byKey(copyKey)).dy,
        closeTo(fenceBottom.dy + 4, 1),
        reason: 'The button belongs below the closing Markdown fence',
      );
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
      expect(find.byKey(copyKey), findsOneWidget);
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
        expect(find.byKey(key), findsOneWidget);
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
      expect(find.byKey(copyKey), findsOneWidget);

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
      expect(find.byKey(copyKey), findsOneWidget);
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
          findsOneWidget,
          reason:
              'Copy actions stay visible when the mouse is over non-code text',
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

  testWidgets('copy button distinguishes Objective-C and Swift source', (
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
          '❯ give me objective-c and swift code\r\n'
          '• Objective-C (queens.m)\r\n'
          '#import <Foundation/Foundation.h>\r\n'
          '@interface QueensSolver : NSObject\r\n'
          '- (void)solve;\r\n'
          '@end\r\n'
          '\r\n'
          'Swift (queens.swift)\r\n'
          'let n = 8\r\n'
          'struct QueensSolver {\r\n'
          '  private mutating func solve() {\r\n'
          '    print("ok")\r\n'
          '  }\r\n'
          '}\r\n'
          '\r\n'
          'These versions use backtracking.\r\n'
          '✻ Churned for 10s',
        ),
      );
      await _pumpTerminalOutput(tester);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(
        location: _cellCenter(tester, const xterm.CellOffset(4, 0)),
      );
      const key = ValueKey<String>('terminal-copy-code-button');
      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 4)));
      await tester.pump();
      expect(
        find.byKey(key),
        findsOneWidget,
        reason: 'Objective-C method should show code copy control',
      );
      pty.writes.clear();
      await tester.tap(find.byKey(key));
      await tester.pump();
      expect(clipboardWrites, <String>[
        '#import <Foundation/Foundation.h>\n'
            '@interface QueensSolver : NSObject\n'
            '- (void)solve;\n@end',
      ]);
      expect(pty.writes, isEmpty);

      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 11)));
      await tester.pump();
      expect(
        find.byKey(key),
        findsOneWidget,
        reason: 'Swift nested method should have its own copy control',
      );
      await tester.tap(find.byKey(key));
      await tester.pump();
      expect(clipboardWrites, <String>[
        '#import <Foundation/Foundation.h>\n'
            '@interface QueensSolver : NSObject\n'
            '- (void)solve;\n@end',
        'let n = 8\nstruct QueensSolver {\n'
            '  private mutating func solve() {\n'
            '    print("ok")\n  }\n}',
      ]);
      expect(pty.writes, isEmpty);

      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 7)));
      await tester.pump();
      expect(
        find.byKey(key),
        findsOneWidget,
        reason:
            'Copy actions stay visible when the mouse is over non-code text',
      );
      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 15)));
      await tester.pump();
      expect(
        find.byKey(key),
        findsOneWidget,
        reason:
            'Copy actions stay visible when the mouse is over non-code text',
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
        findsOneWidget,
        reason:
            'Copy actions stay visible when the mouse is over non-code text',
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

  testWidgets('copies complete C source starting with Claude code bullet', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.windows;
    final copies = <String>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copies.add(
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
          '❯ 8 queens in C\r\n'
          '• #include <stdio.h>\r\n'
          '  \r\n'
          '  #define N 8\r\n'
          '  \r\n'
          '  static int queens[N];\r\n'
          '  \r\n'
          '  void solve(int row)\r\n'
          '  {\r\n'
          '    if (row == N) {\r\n'
          '      printf("Solved");\r\n'
          '      return;\r\n'
          '    }\r\n'
          '  }\r\n'
          'How it works: backtracking.\r\n'
          '✻ Churned for 6s',
        ),
      );
      await _pumpTerminalOutput(tester);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(
        location: _cellCenter(tester, const xterm.CellOffset(4, 0)),
      );
      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 10)));
      await tester.pump();
      const button = ValueKey<String>('terminal-copy-code-button');
      expect(find.byKey(button), findsOneWidget);
      pty.writes.clear();
      await tester.tap(find.byKey(button));
      await tester.pump();
      const expected =
          '#include <stdio.h>\n\n#define N 8\n\n'
          'static int queens[N];\n\nvoid solve(int row)\n'
          '{\n  if (row == N) {\n    printf("Solved");\n'
          '    return;\n  }\n}';
      expect(copies, <String>[expected]);
      expect(pty.writes, isEmpty);
      await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(4, 14)));
      await tester.pump();
      expect(
        find.byKey(button),
        findsOneWidget,
        reason:
            'Copy actions stay visible when the mouse is over non-code text',
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
    'copy button hides ten seconds after leaving code, not during the click',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.windows;
      final copied = <String>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.setData') {
            copied.add(
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
        factory.sessions.single.emitOutput(
          utf8.encode('~~~dart\r\nvoid main() {}\r\n~~~\r\nDone'),
        );
        await _pumpTerminalOutput(tester);
        const key = ValueKey<String>('terminal-copy-code-button');
        expect(find.byKey(key), findsOneWidget);

        final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
        await mouse.addPointer(
          location: _cellCenter(tester, const xterm.CellOffset(3, 1)),
        );
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 3)));
        await tester.pump();
        await tester.pump(const Duration(seconds: 9));
        expect(
          find.byKey(key),
          findsOneWidget,
          reason: 'Leaving source lines must not immediately remove copy',
        );

        await mouse.moveTo(tester.getCenter(find.byKey(key)));
        await tester.pump();
        await tester.pump(const Duration(seconds: 11));
        expect(
          find.byKey(key),
          findsOneWidget,
          reason: 'Time spent over the button must not dismiss it',
        );

        await mouse.down(tester.getCenter(find.byKey(key)));
        await mouse.up();
        await tester.pump();
        expect(copied, <String>['void main() {}']);

        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 3)));
        await tester.pump();
        await tester.pump(const Duration(seconds: 9));
        expect(find.byKey(key), findsOneWidget);
        await tester.pump(const Duration(seconds: 1));
        expect(
          find.byKey(key),
          findsNothing,
          reason: 'Ten seconds outside code and button dismisses it',
        );

        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 1)));
        await tester.pump();
        expect(
          find.byKey(key),
          findsOneWidget,
          reason: 'Returning to code must reveal the button again',
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

  testWidgets(
    'copy buttons default on, persist across mouse paths, and toggle live',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.windows;
      final copied = <String>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.setData') {
            copied.add(
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
            'Claude says:\r\n'
            '~~~dart\r\n'
            'void main() {}\r\n'
            '~~~\r\n'
            'Done',
          ),
        );
        await _pumpTerminalOutput(tester);

        const copyKey = ValueKey<String>('terminal-copy-code-button');
        expect(runtime.terminalSettings.showCodeBlockCopyButton, isTrue);
        expect(
          find.byKey(copyKey),
          findsOneWidget,
          reason: 'A recognized code block gets a button before any hover',
        );

        final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
        await mouse.addPointer(
          location: _cellCenter(tester, const xterm.CellOffset(3, 2)),
        );
        await mouse.moveTo(_cellCenter(tester, const xterm.CellOffset(3, 4)));
        await tester.pump();
        expect(
          find.byKey(copyKey),
          findsOneWidget,
          reason: 'Passing over non-code spacer rows must not hide the button',
        );
        await mouse.moveTo(tester.getCenter(find.byKey(copyKey)));
        await tester.pump();
        await mouse.down(tester.getCenter(find.byKey(copyKey)));
        await mouse.up();
        await tester.pump();
        expect(copied, <String>['void main() {}']);
        expect(pty.writes, isEmpty);

        runtime.updateSettings(
          TerminalSettings.defaults.copyWith(showCodeBlockCopyButton: false),
        );
        await tester.pump();
        expect(find.byKey(copyKey), findsNothing);

        runtime.updateSettings(TerminalSettings.defaults);
        await tester.pump();
        await tester.pump();
        expect(find.byKey(copyKey), findsOneWidget);
        expect(pty.writes, isEmpty);
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
