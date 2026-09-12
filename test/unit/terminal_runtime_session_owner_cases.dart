part of 'terminal_runtime_native_test.dart';

void _registerTerminalRuntimeSessionOwnerTests() {
  test(
    'two tabs can attach to one durable session and release independently',
    () async {
      debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
      final factory = _SharedTerminalPtySessionFactory();
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: factory,
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      try {
        final first = runtime.sessionFor(
          workspace: _workspace(),
          tab: _tab(
            id: 'tab-1',
            payload: const <String, Object?>{
              workspaceTabTerminalSessionIdPayloadKey: 'durable-session',
            },
          ),
        );
        final second = runtime.sessionFor(
          workspace: _workspace(),
          tab: _tab(
            id: 'tab-2',
            title: 'Terminal 2',
            payload: const <String, Object?>{
              workspaceTabTerminalSessionIdPayloadKey: 'durable-session',
            },
          ),
        );
        final firstVisibility = acquireTerminalVisibilityForTesting(first);
        final secondVisibility = acquireTerminalVisibilityForTesting(second);
        addTearDown(firstVisibility.dispose);
        addTearDown(secondVisibility.dispose);

        await Future.wait(<Future<void>>[
          first.ensureStarted(),
          second.ensureStarted(),
        ]);

        final durable = factory.stateFor('durable-session');
        durable.emitOutput(utf8.encode('shared-output\r\n'));
        await Future.pause(.zero);
        flushTerminalOutputForTesting(first);
        flushTerminalOutputForTesting(second);

        expect(terminalBufferTextForTesting(first), contains('shared-output'));
        expect(terminalBufferTextForTesting(second), contains('shared-output'));

        runtime.releaseTab('tab-1');
        await Future.pause(.zero);
        expect(durable.terminated, isFalse);
        expect(runtime.peekSession('tab-1'), isNull);
        expect(runtime.peekSession('tab-2'), same(second));

        durable.emitOutput(utf8.encode('after-release\r\n'));
        await Future.pause(.zero);
        flushTerminalOutputForTesting(second);
        expect(terminalBufferTextForTesting(second), contains('after-release'));

        runtime.closeTab('tab-2');
        await Future.pause(.zero);
        expect(durable.terminated, isTrue);
      } finally {
        debugDefaultTargetPlatformOverride = null;
      }
    },
  );

  test(
    'eviction reattaches a durable session and replays its snapshot',
    () async {
      debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
      final original = _FakeTerminalPtySession();
      final replacement = _SnapshotOnStartTerminalPtySession(
        utf8.encode('durable-scrollback\r\n'),
      );
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: _FakeTerminalPtySessionFactory(
          sessions: <_FakeTerminalPtySession>[original, replacement],
        ),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final tab = _tab(
        id: 'tab-1',
        payload: const <String, Object?>{
          workspaceTabTerminalSessionIdPayloadKey: 'durable-session',
        },
      );
      try {
        final session = runtime.sessionFor(workspace: _workspace(), tab: tab);
        await _startFillAndHide(session);
        runtime.setActiveWorkspace('workspace-2');
        runtime.updateSettings(
          TerminalSettings.defaults.copyWith(bufferBudgetMegabytes: 1),
        );

        expect(runtime.peekSession('tab-1'), isNull);
        expect(original.terminated, isFalse);

        final reattached = runtime.sessionFor(
          workspace: _workspace(),
          tab: tab,
        );
        final visibility = acquireTerminalVisibilityForTesting(reattached);
        addTearDown(visibility.dispose);
        await reattached.ensureStarted();
        await Future.pause(.zero);
        flushTerminalOutputForTesting(reattached);

        expect(
          terminalBufferTextForTesting(reattached),
          contains('durable-scrollback'),
        );
        expect(replacement.terminated, isFalse);
      } finally {
        debugDefaultTargetPlatformOverride = null;
      }
    },
  );

  test('runtime dispose returns before a slow PTY detach completes', () async {
    debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
    final pty = _SlowCancelTerminalPtySession();
    final runtime = XtermTerminalRuntime(
      ptySessionFactory: _SingleTerminalPtySessionFactory(pty),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        _launch('shell', shell: '/bin/sh'),
      ],
    );
    try {
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      await session.ensureStarted();

      runtime.dispose();

      expect(pty.cancelRequested, isTrue);
      expect(pty.disposed, isFalse);

      pty.allowCancel.complete();
      await _settleUntil(() => pty.disposed);
      expect(pty.disposed, isTrue);
      expect(pty.terminated, isFalse);
    } finally {
      debugDefaultTargetPlatformOverride = null;
    }
  });

  testWidgets(
    'a disposed session ignores a restore completion scheduled before disposal',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
      final fakeSession = _FakeTerminalPtySession();
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: _FakeTerminalPtySessionFactory(
          sessions: <_FakeTerminalPtySession>[fakeSession],
        ),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      final visibility = acquireTerminalVisibilityForTesting(session);
      try {
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: AnimatedBuilder(
                animation: session,
                builder: (context, _) => session.buildView(),
              ),
            ),
          ),
        );
        await tester.pump();
        await session.ensureStarted();
        await tester.pump(const Duration(milliseconds: 200));
        fakeSession.resizeCalls.clear();
        fakeSession.emitSnapshot(utf8.encode('restore-tail\r\n'));
        await tester.idle();
        while (pendingRestoreTerminalOutputCharsForTesting(session) > 0) {
          flushTerminalOutputForTesting(session);
        }

        runtime.dispose();
        await tester.pump();

        expect(fakeSession.resizeCalls, isEmpty);
        expect(tester.takeException(), isNull);
      } finally {
        visibility.dispose();
        runtime.dispose();
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.pump();
        debugDefaultTargetPlatformOverride = null;
      }
    },
  );
}

final class _SharedTerminalPtySessionFactory
    implements TerminalPtySessionFactory {
  final Map<String, _SharedTerminalPtyState> _states =
      <String, _SharedTerminalPtyState>{};

  _SharedTerminalPtyState stateFor(String sessionId) => _states[sessionId]!;

  @override
  TerminalPtySession create({
    required String sessionId,
    required String workspaceId,
    required String tabId,
  }) {
    final state = _states.putIfAbsent(sessionId, _SharedTerminalPtyState.new);
    return _SharedTerminalPtySession(state);
  }
}

final class _SharedTerminalPtyState {
  final StreamController<TerminalPtySessionEvent> events =
      StreamController<TerminalPtySessionEvent>.broadcast();
  bool terminated = false;

  void emitOutput(List<int> data) {
    if (!events.isClosed) {
      events.add(TerminalPtyOutputEvent(Uint8List.fromList(data)));
    }
  }
}

final class _SharedTerminalPtySession implements TerminalPtySession {
  _SharedTerminalPtySession(this._state);

  final _SharedTerminalPtyState _state;
  bool _disposed = false;

  @override
  Stream<TerminalPtySessionEvent> get events => _state.events.stream;

  @override
  bool get startedNewProcess => true;

  @override
  Future<void> start({
    required GhosttyTerminalShellLaunch launch,
    required String workingDirectory,
    required int cols,
    required int rows,
    Future<void> Function()? onProcessCreated,
  }) async {
    await onProcessCreated?.call();
  }

  @override
  bool writeBytes(List<int> bytes) => !_disposed && bytes.isNotEmpty;

  @override
  Future<bool> writeBytesAndWait(List<int> bytes) async => writeBytes(bytes);

  @override
  void resize(int cols, int rows, int cellWidthPx, int cellHeightPx) {}

  @override
  Future<void> refreshViewport(
    int cols,
    int rows,
    int cellWidthPx,
    int cellHeightPx,
  ) async {}

  @override
  Future<void> setOutputPaused(bool paused) async {}

  @override
  void dispose() {
    _disposed = true;
  }

  @override
  void terminate() {
    _disposed = true;
    _state.terminated = true;
    unawaited(_state.events.close());
  }
}

final class _SnapshotOnStartTerminalPtySession extends _FakeTerminalPtySession {
  _SnapshotOnStartTerminalPtySession(this._snapshot);

  final List<int> _snapshot;

  @override
  Future<void> start({
    required GhosttyTerminalShellLaunch launch,
    required String workingDirectory,
    required int cols,
    required int rows,
    Future<void> Function()? onProcessCreated,
  }) async {
    await super.start(
      launch: launch,
      workingDirectory: workingDirectory,
      cols: cols,
      rows: rows,
      onProcessCreated: onProcessCreated,
    );
    emitSnapshot(_snapshot);
  }
}

final class _SlowCancelTerminalPtySession implements TerminalPtySession {
  _SlowCancelTerminalPtySession() {
    _events = StreamController<TerminalPtySessionEvent>(
      onCancel: () async {
        cancelRequested = true;
        await allowCancel.future;
      },
    );
  }

  final Completer<void> allowCancel = Completer<void>();
  late final StreamController<TerminalPtySessionEvent> _events;
  bool cancelRequested = false;
  bool disposed = false;
  bool terminated = false;

  @override
  Stream<TerminalPtySessionEvent> get events => _events.stream;

  @override
  bool get startedNewProcess => true;

  @override
  Future<void> start({
    required GhosttyTerminalShellLaunch launch,
    required String workingDirectory,
    required int cols,
    required int rows,
    Future<void> Function()? onProcessCreated,
  }) async {
    await onProcessCreated?.call();
  }

  @override
  bool writeBytes(List<int> bytes) => !disposed && bytes.isNotEmpty;

  @override
  Future<bool> writeBytesAndWait(List<int> bytes) async => writeBytes(bytes);

  @override
  void resize(int cols, int rows, int cellWidthPx, int cellHeightPx) {}

  @override
  Future<void> refreshViewport(
    int cols,
    int rows,
    int cellWidthPx,
    int cellHeightPx,
  ) async {}

  @override
  Future<void> setOutputPaused(bool paused) async {}

  @override
  void dispose() {
    if (disposed) {
      return;
    }
    disposed = true;
    unawaited(_events.close());
  }

  @override
  void terminate() {
    terminated = true;
    dispose();
  }
}

final class _SingleTerminalPtySessionFactory
    implements TerminalPtySessionFactory {
  _SingleTerminalPtySessionFactory(this._session);

  final TerminalPtySession _session;
  bool _created = false;

  @override
  TerminalPtySession create({
    required String sessionId,
    required String workspaceId,
    required String tabId,
  }) {
    if (_created) {
      throw StateError('Only one PTY session is available for this test.');
    }
    _created = true;
    return _session;
  }
}
