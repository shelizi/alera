part of 'terminal_surface_test.dart';

void _registerTerminalSurfaceTabSwitchTests() {
  testWidgets('Terminal follows live output after a resized view handoff', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    try {
      final factory = _FakeTerminalPtySessionFactory();
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: factory,
        shellLaunchesBuilder: _testShellLaunches,
      );
      addTearDown(runtime.dispose);
      final workspace = _workspace();
      final tab = _tab();
      final session = runtime.sessionFor(workspace: workspace, tab: tab);

      Future<void> mountAtHeight(double height) async {
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: Align(
                alignment: Alignment.topLeft,
                child: SizedBox(
                  width: 640,
                  height: height,
                  child: TerminalSurface(session: session),
                ),
              ),
            ),
          ),
        );
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 200));
      }

      await mountAtHeight(260);
      final pty = factory.sessions.single;
      pty.emitOutput(
        utf8.encode(
          [for (var i = 0; i < 100; i++) 'VIEW_START_$i'].join('\r\n'),
        ),
      );
      await _pumpTerminalOutput(tester);
      var scroll = tester
          .widget<xterm.TerminalView>(find.byType(xterm.TerminalView))
          .scrollController!;
      expect(scroll.position.extentAfter, lessThan(4));

      // The old view was at the tail. While there is no view, keep writing;
      // then attach the same session in a larger viewport and add more data.
      await _hideTerminalSurface(tester);
      pty.emitOutput(
        utf8.encode(
          [for (var i = 100; i < 145; i++) 'VIEW_GAP_$i'].join('\r\n'),
        ),
      );
      await mountAtHeight(440);
      pty.emitOutput(utf8.encode('\r\nVIEW_AFTER_RETURN\r\n'));
      await _pumpTerminalOutput(tester);
      scroll = tester
          .widget<xterm.TerminalView>(find.byType(xterm.TerminalView))
          .scrollController!;

      expect(
        terminalBufferTextForTesting(session),
        contains('VIEW_AFTER_RETURN'),
      );
      expect(scroll.position.extentAfter, lessThan(4));
      expect(pty.terminated, isFalse);
      expect(factory.sessions, hasLength(1));
    } finally {
      debugDefaultTargetPlatformOverride = null;
    }
  });

  testWidgets('Continuous PTY output survives source and child view handoffs', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    try {
      final factory = _FakeTerminalPtySessionFactory();
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: factory,
        shellLaunchesBuilder: _testShellLaunches,
      );
      addTearDown(runtime.dispose);
      final workspace = _workspace();
      final tab = _tab();
      final session = runtime.sessionFor(workspace: workspace, tab: tab);

      await _pumpTerminalSurface(tester, session);
      final pty = factory.sessions.single;
      pty.emitOutput(utf8.encode('STREAM_BEGIN\r\n'));
      await _pumpTerminalOutput(tester);

      // Output must not kill the durable PTY during the brief gap between
      // unmounting the source and mounting the new native Flutter view.
      await _hideTerminalSurface(tester);
      for (var index = 0; index < 45; index++) {
        pty.emitOutput(utf8.encode('STREAM_GAP_$index\r\n'));
      }
      expect(runtime.peekSession(tab.id), same(session));
      expect(pty.terminated, isFalse);

      await _pumpTerminalSurface(
        tester,
        runtime.sessionFor(workspace: workspace, tab: tab),
      );
      pty.emitOutput(utf8.encode('STREAM_CHILD\r\n'));
      await _pumpTerminalOutput(tester);
      expect(terminalBufferTextForTesting(session), contains('STREAM_CHILD'));

      // The child may close while stdout is still being delivered: the
      // source must receive both the gap and the latest output after remount.
      await _hideTerminalSurface(tester);
      for (var index = 0; index < 45; index++) {
        pty.emitOutput(utf8.encode('STREAM_RETURN_$index\r\n'));
      }
      await _pumpTerminalSurface(
        tester,
        runtime.sessionFor(workspace: workspace, tab: tab),
      );
      pty.emitOutput(utf8.encode('STREAM_END\r\n'));
      await _pumpTerminalOutput(tester);

      final buffer = terminalBufferTextForTesting(session);
      expect(buffer, contains('STREAM_GAP_44'));
      expect(buffer, contains('STREAM_RETURN_44'));
      expect(buffer, contains('STREAM_END'));
      expect(pty.terminated, isFalse);
      expect(factory.sessions, hasLength(1));
      expect(runtime.peekSession(tab.id), same(session));
    } finally {
      debugDefaultTargetPlatformOverride = null;
    }
  });

  testWidgets('Terminal view handoff keeps its PTY and output history', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    try {
      final factory = _FakeTerminalPtySessionFactory();
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: factory,
        shellLaunchesBuilder: _testShellLaunches,
      );
      addTearDown(runtime.dispose);
      final workspace = _workspace();
      final tab = _tab();
      final session = runtime.sessionFor(workspace: workspace, tab: tab);

      await _pumpTerminalSurface(tester, session);
      final pty = factory.sessions.single;
      pty.emitOutput(utf8.encode('persistent-terminal-output\r\n'));
      await _pumpTerminalOutput(tester);
      expect(
        terminalBufferTextForTesting(session),
        contains('persistent-terminal-output'),
      );

      // A shared child may only attach after the source has unmounted.
      await _hideTerminalSurface(tester);
      expect(runtime.peekSession(tab.id), same(session));
      expect(pty.terminated, isFalse);
      final detachedSession = runtime.sessionFor(
        workspace: workspace,
        tab: tab,
      );
      expect(detachedSession, same(session));
      await _pumpTerminalSurface(tester, detachedSession);
      expect(factory.sessions, hasLength(1));

      // Returning from a native child close must reuse the same live PTY.
      await _hideTerminalSurface(tester);
      await _pumpTerminalSurface(
        tester,
        runtime.sessionFor(workspace: workspace, tab: tab),
      );
      expect(factory.sessions, hasLength(1));
      expect(pty.terminated, isFalse);
      expect(
        terminalBufferTextForTesting(session),
        contains('persistent-terminal-output'),
      );
    } finally {
      debugDefaultTargetPlatformOverride = null;
    }
  });

  testWidgets(
    'a resumed shell does not receive a click before TUI cleanup is parsed',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.linux;
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
        pty.emitOutput(utf8.encode('\x1b[?1000h\x1b[?1006h'));
        await _pumpTerminalOutput(tester);
        expect(
          terminalMouseModeForTesting(session),
          isNot(xterm.MouseMode.none),
        );
        pty.writes.clear();

        await _hideTerminalSurface(tester);
        // Hidden tabs intentionally keep PTY output flowing. A full app
        // background transition is what pauses host delivery now.
        runtime.setAppForeground(false);
        expect(pty.outputPausedCalls.last, isTrue);
        pty.emitOutput(utf8.encode('\x1b[?1000l\x1b[?1006l\r\nshell prompt'));
        await tester.pump();
        expect(
          terminalMouseModeForTesting(session),
          isNot(xterm.MouseMode.none),
        );

        final resume = Completer<void>();
        pty.nextResumeCompleter = resume;
        await tester.pumpWidget(_terminalSurfaceFixture(session));
        await tester.pump();
        runtime.setAppForeground(true);
        expect(pty.outputPausedCalls.last, isFalse);
        expect(terminalPointerInputSuspendedForTesting(session), isTrue);

        await tester.tapAt(_cellCenter(tester, const xterm.CellOffset(2, 0)));
        await tester.pump(const Duration(milliseconds: 300));

        expect(pty.writes, isEmpty);

        resume.complete();
        await tester.pump();
        await _pumpTerminalOutput(tester);

        expect(terminalMouseModeForTesting(session), xterm.MouseMode.none);
        expect(terminalPointerInputSuspendedForTesting(session), isFalse);
      } finally {
        debugDefaultTargetPlatformOverride = null;
      }
    },
  );

  testWidgets(
    'a live TUI receives pointer input after resume synchronization',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.linux;
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
        pty.emitOutput(utf8.encode('\x1b[?1000h\x1b[?1006h'));
        await _pumpTerminalOutput(tester);
        pty.writes.clear();

        await _hideTerminalSurface(tester);
        runtime.setAppForeground(false);
        expect(pty.outputPausedCalls.last, isTrue);
        final resume = Completer<void>();
        pty.nextResumeCompleter = resume;
        await tester.pumpWidget(_terminalSurfaceFixture(session));
        await tester.pump();
        runtime.setAppForeground(true);

        await tester.tapAt(_cellCenter(tester, const xterm.CellOffset(2, 0)));
        await tester.pump(const Duration(milliseconds: 300));
        expect(pty.writes, isEmpty);

        resume.complete();
        await tester.pump();
        await tester.pump();
        expect(terminalPointerInputSuspendedForTesting(session), isFalse);

        await tester.tapAt(_cellCenter(tester, const xterm.CellOffset(2, 0)));
        await tester.pump(const Duration(milliseconds: 300));

        expect(
          utf8.decode(pty.writes.expand((bytes) => bytes).toList()),
          contains('\x1b[<0;'),
        );
      } finally {
        debugDefaultTargetPlatformOverride = null;
      }
    },
  );

  testWidgets('a stale resume cannot enable pointer input while hidden', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
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
      await _hideTerminalSurface(tester);
      final resume = Completer<void>();
      pty.nextResumeCompleter = resume;
      await tester.pumpWidget(_terminalSurfaceFixture(session));
      await tester.pump();
      await _hideTerminalSurface(tester);

      resume.complete();
      await tester.pump();
      await tester.pump();

      expect(terminalPointerInputSuspendedForTesting(session), isTrue);
      await tester.pump(const Duration(milliseconds: 200));
    } finally {
      debugDefaultTargetPlatformOverride = null;
    }
  });
}

Widget _terminalSurfaceFixture(TerminalSessionHandle session) {
  return MaterialApp(
    home: Scaffold(
      body: SizedBox.expand(child: TerminalSurface(session: session)),
    ),
  );
}

Future<void> _hideTerminalSurface(WidgetTester tester) async {
  await tester.pumpWidget(const MaterialApp(home: SizedBox.shrink()));
  await tester.pump();
}
