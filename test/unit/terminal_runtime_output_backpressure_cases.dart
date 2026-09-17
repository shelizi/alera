part of 'terminal_runtime_native_test.dart';

void _registerTerminalRuntimeOutputBackpressureTests() {
  test('parser worker applies output into the xterm replica', () async {
    final runtime = XtermTerminalRuntime(
      parserWorkerEnabled: true,
      ptySessionFactory: _FakeTerminalPtySessionFactory(),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        _launch('shell', shell: '/bin/sh'),
      ],
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());

    expect(terminalParserWorkerEnabledForTesting(session), isTrue);
    queueTerminalOutputForTesting(session, 'worker-runtime-marker\x1b[?2004h');
    flushTerminalOutputForTesting(session);
    await waitForTerminalParserApplyForTesting(session);

    expect(
      terminalBufferTextForTesting(session),
      contains('worker-runtime-marker'),
    );
    expect(terminalBracketedPasteModeForTesting(session), isTrue);
  });

  test('parser worker resizes before parsing following output', () async {
    final runtime = XtermTerminalRuntime(
      parserWorkerEnabled: true,
      ptySessionFactory: _FakeTerminalPtySessionFactory(),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        _launch('shell', shell: '/bin/sh'),
      ],
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());

    queueTerminalOutputForTesting(session, 'warmup\r\n');
    flushTerminalOutputForTesting(session);
    await waitForTerminalParserApplyForTesting(session);

    resizeTerminalForTesting(session, 6, 4);
    queueTerminalOutputForTesting(session, '1234567890');
    flushTerminalOutputForTesting(session);
    await waitForTerminalParserApplyForTesting(session);

    expect(terminalBufferTextForTesting(session), contains('123456\n7890'));
  });

  test('starts adaptive parsing at the measured TUI-safe chunk size', () {
    final runtime = XtermTerminalRuntime(
      ptySessionFactory: _FakeTerminalPtySessionFactory(),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        _launch('shell', shell: '/bin/sh'),
      ],
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());

    expect(terminalOutputAdaptiveBudgetCharsForTesting(session), 16 * 1024);
  });

  test('adaptive parse budget shrinks when xterm parsing exceeds target', () {
    expect(
      terminalOutputAdaptiveBudgetForTesting(
        currentChars: 64 * 1024,
        parseTime: const Duration(milliseconds: 12),
      ),
      32 * 1024,
    );
    expect(
      terminalOutputAdaptiveBudgetForTesting(
        currentChars: 8 * 1024,
        parseTime: const Duration(milliseconds: 24),
      ),
      4 * 1024,
    );
  });

  test('adaptive parse budget grows conservatively after cheap parsing', () {
    expect(
      terminalOutputAdaptiveBudgetForTesting(
        currentChars: 16 * 1024,
        parseTime: const Duration(milliseconds: 2),
      ),
      24 * 1024,
    );
    expect(
      terminalOutputAdaptiveBudgetForTesting(
        currentChars: 64 * 1024,
        parseTime: const Duration(milliseconds: 1),
      ),
      64 * 1024,
    );
    expect(
      terminalOutputAdaptiveBudgetForTesting(
        currentChars: 32 * 1024,
        parseTime: const Duration(milliseconds: 5),
      ),
      32 * 1024,
    );
  });

  test(
    'bounds hidden overflow parsing instead of draining the whole backlog',
    () {
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: _FakeTerminalPtySessionFactory(),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());

      // The prefix must still reach xterm so terminal state is preserved, but
      // an oversized hidden burst must not be parsed synchronously in one shot
      // on the Flutter UI isolate.
      queueTerminalOutputForTesting(
        session,
        '\x1b[?2004h${'a' * (1024 * 1024 + 64 * 1024)}',
      );

      expect(pendingTerminalOutputCharsForTesting(session), greaterThan(0));
      expect(terminalBracketedPasteModeForTesting(session), isTrue);
    },
  );

  test(
    'visible overflow catches up on a frame instead of the output callback',
    () {
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: _FakeTerminalPtySessionFactory(),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      final visibility = acquireTerminalVisibilityForTesting(session);
      addTearDown(visibility.dispose);

      queueTerminalOutputForTesting(
        session,
        '\x1b[?2004h${'a' * (1536 * 1024)}',
      );

      // Medium visible bursts should not synchronously parse on the host-output
      // callback. They are promoted to the next frame instead, so repeated host
      // frames cannot each spend a parse budget on the Flutter UI isolate.
      expect(terminalBracketedPasteModeForTesting(session), isFalse);
      expect(
        pendingLiveTerminalOutputCharsForTesting(session),
        greaterThan(1024 * 1024),
      );
      expect(terminalOutputFlushScheduledForTesting(session), isTrue);
      expect(terminalOutputFlushDeferredForTesting(session), isFalse);

      flushTerminalOutputForTesting(session);
      expect(terminalBracketedPasteModeForTesting(session), isTrue);
    },
  );

  test(
    'extreme visible overflow keeps one bounded synchronous safety drain',
    () {
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: _FakeTerminalPtySessionFactory(),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      final visibility = acquireTerminalVisibilityForTesting(session);
      addTearDown(visibility.dispose);

      final payload = '\x1b[?2004h${'a' * (5 * 1024 * 1024)}';
      queueTerminalOutputForTesting(session, payload);

      expect(terminalBracketedPasteModeForTesting(session), isTrue);
      expect(
        pendingLiveTerminalOutputCharsForTesting(session),
        lessThan(payload.length),
      );
      expect(
        pendingLiveTerminalOutputCharsForTesting(session),
        greaterThan(4 * 1024 * 1024),
      );
    },
  );

  test('hidden overflow catch-up yields down to a low-water backlog', () async {
    final runtime = XtermTerminalRuntime(
      ptySessionFactory: _FakeTerminalPtySessionFactory(),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        _launch('shell', shell: '/bin/sh'),
      ],
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());

    queueTerminalOutputForTesting(session, 'a' * (1536 * 1024));

    // Hidden catch-up is intentionally event-loop paced instead of one giant
    // synchronous parse. Give even the 4 KiB minimum adaptive budget enough
    // turns to reach the intended low-water mark.
    for (
      var turn = 0;
      turn < 512 &&
          pendingLiveTerminalOutputCharsForTesting(session) > 256 * 1024;
      turn++
    ) {
      await Future<void>.delayed(Duration.zero);
    }

    expect(
      pendingLiveTerminalOutputCharsForTesting(session),
      lessThanOrEqualTo(256 * 1024),
    );
    expect(pendingLiveTerminalOutputCharsForTesting(session), greaterThan(0));
  });

  test('hidden catch-up coalesces terminal listeners until reveal', () async {
    final runtime = XtermTerminalRuntime(
      ptySessionFactory: _FakeTerminalPtySessionFactory(),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        _launch('shell', shell: '/bin/sh'),
      ],
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    var notifications = 0;
    final removeListener = addTerminalChangeListenerForTesting(
      session,
      () => notifications += 1,
    );
    addTearDown(removeListener);

    queueTerminalOutputForTesting(
      session,
      'hidden-listener-marker${'a' * (1536 * 1024)}',
    );
    for (
      var turn = 0;
      turn < 512 &&
          pendingLiveTerminalOutputCharsForTesting(session) > 256 * 1024;
      turn++
    ) {
      await Future<void>.delayed(Duration.zero);
    }

    expect(notifications, 0);

    final visibility = acquireTerminalVisibilityForTesting(session);
    addTearDown(visibility.dispose);

    expect(notifications, greaterThan(0));
  });

  test('hidden catch-up refreshes an active search once on reveal', () async {
    final runtime = XtermTerminalRuntime(
      ptySessionFactory: _FakeTerminalPtySessionFactory(),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        _launch('shell', shell: '/bin/sh'),
      ],
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    final searchController = session.searchController!;
    searchController
      ..open()
      ..setQuery('hidden-search-marker');
    expect(searchController.matchCount, 0);

    final payload =
        '${'a' * (1150 * 1024)}\r\nhidden-search-marker\r\n'
        '${'b' * (386 * 1024)}';
    queueTerminalOutputForTesting(session, payload);
    for (
      var turn = 0;
      turn < 512 &&
          pendingLiveTerminalOutputCharsForTesting(session) > 256 * 1024;
      turn++
    ) {
      await Future<void>.delayed(Duration.zero);
    }

    expect(searchController.matchCount, 0);

    final visibility = acquireTerminalVisibilityForTesting(session);
    addTearDown(visibility.dispose);

    expect(searchController.matchCount, greaterThan(0));
  });

  test(
    'revealing a hidden terminal only parses one UI budget synchronously',
    () {
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: _FakeTerminalPtySessionFactory(),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());

      const frame = 64 * 1024;
      queueTerminalOutputForTesting(
        session,
        'hidden-before-reveal\r\n${'a' * (frame * 3)}hidden-tail\r\n',
      );
      expect(pendingTerminalOutputCharsForTesting(session), greaterThan(0));
      expect(
        terminalBufferTextForTesting(session),
        isNot(contains('hidden-before-reveal')),
      );

      final visibility = acquireTerminalVisibilityForTesting(session);
      addTearDown(visibility.dispose);

      expect(pendingTerminalOutputCharsForTesting(session), greaterThan(0));
      expect(
        terminalBufferTextForTesting(session),
        contains('hidden-before-reveal'),
      );
      expect(
        terminalBufferTextForTesting(session),
        isNot(contains('hidden-tail')),
      );

      while (pendingTerminalOutputCharsForTesting(session) > 0) {
        flushTerminalOutputForTesting(session);
      }
      expect(terminalBufferTextForTesting(session), contains('hidden-tail'));
    },
  );

  test('revealing a terminal resets a hidden learned parse budget', () {
    final runtime = XtermTerminalRuntime(
      ptySessionFactory: _FakeTerminalPtySessionFactory(),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        _launch('shell', shell: '/bin/sh'),
      ],
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());

    setTerminalOutputAdaptiveBudgetForTesting(session, 64 * 1024);
    queueTerminalOutputForTesting(session, 'a' * (64 * 1024));

    final visibility = acquireTerminalVisibilityForTesting(session);
    addTearDown(visibility.dispose);

    expect(terminalOutputAdaptiveBudgetCharsForTesting(session), 16 * 1024);
    expect(pendingTerminalOutputCharsForTesting(session), 48 * 1024);
  });

  test(
    'an idle terminal restarts a learned parse budget conservatively',
    () async {
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: _FakeTerminalPtySessionFactory(),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      final visibility = acquireTerminalVisibilityForTesting(session);
      addTearDown(visibility.dispose);

      queueTerminalOutputForTesting(session, 'warmup');
      flushTerminalOutputForTesting(session);
      setTerminalOutputAdaptiveBudgetForTesting(session, 64 * 1024);

      await Future<void>.delayed(
        terminalOutputAdaptiveIdleResetIntervalForTesting +
            const Duration(milliseconds: 50),
      );
      queueTerminalOutputForTesting(session, 'new-burst');

      expect(terminalOutputAdaptiveBudgetCharsForTesting(session), 16 * 1024);
    },
  );

  test('drains a large chunk without recopying the pending head', () {
    final runtime = XtermTerminalRuntime(
      ptySessionFactory: _FakeTerminalPtySessionFactory(),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        _launch('shell', shell: '/bin/sh'),
      ],
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());

    final frame = terminalOutputAdaptiveBudgetCharsForTesting(session);
    queueTerminalOutputForTesting(session, 'a' * (frame * 3));
    final queued = pendingTerminalOutputHeadChunkForTesting(session);

    for (var drained = 1; drained < 3; drained++) {
      flushTerminalOutputForTesting(session);
      expect(
        pendingTerminalOutputCharsForTesting(session),
        frame * (3 - drained),
      );
      expect(pendingTerminalOutputHeadForTesting(session), frame * drained);
      // The remainder must be the very same string, not a fresh substring:
      // re-queueing the tail is what made a snapshot replay quadratic.
      expect(
        identical(pendingTerminalOutputHeadChunkForTesting(session), queued),
        isTrue,
      );
    }

    flushTerminalOutputForTesting(session);
    expect(pendingTerminalOutputCharsForTesting(session), 0);
    expect(pendingTerminalOutputHeadChunkForTesting(session), isNull);
    expect(pendingTerminalOutputHeadForTesting(session), 0);
  });

  test(
    'process exit does not synchronously drain an oversized backlog',
    () async {
      final fakeSession = _FakeTerminalPtySession();
      final factory = _FakeTerminalPtySessionFactory(
        sessions: <_FakeTerminalPtySession>[fakeSession],
      );
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: factory,
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      await session.ensureStarted();

      const frame = 64 * 1024;
      queueTerminalOutputForTesting(session, 'a' * (frame * 3));
      fakeSession.emitExit(0);
      await Future.pause(.zero);

      expect(
        pendingTerminalOutputCharsForTesting(session),
        greaterThanOrEqualTo(frame * 2),
      );
    },
  );

  test('writes a multi-frame chunk through intact', () {
    final runtime = XtermTerminalRuntime(
      ptySessionFactory: _FakeTerminalPtySessionFactory(),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        _launch('shell', shell: '/bin/sh'),
      ],
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());

    // The emoji straddles the 64 KiB frame boundary, so the head offset has to
    // back off a code unit without losing or duplicating it.
    queueTerminalOutputForTesting(
      session,
      '${'a' * (64 * 1024 - 1)}😀\r\ntail line\r\n',
    );
    while (pendingTerminalOutputCharsForTesting(session) > 0) {
      flushTerminalOutputForTesting(session);
    }

    final text = terminalBufferTextForTesting(session);
    expect(text, contains('😀'));
    expect(text, contains('tail line'));
  });

  test(
    'paces flushes under sustained output instead of one per frame',
    () async {
      final runtime = XtermTerminalRuntime(
        ptySessionFactory: _FakeTerminalPtySessionFactory(),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      final visibility = acquireTerminalVisibilityForTesting(session);
      addTearDown(visibility.dispose);

      // A terminal that has been quiet flushes on the very next frame: the
      // cadence floor must not add latency to an echoed keystroke.
      queueTerminalOutputForTesting(session, 'first\r\n');
      expect(terminalOutputFlushScheduledForTesting(session), isTrue);
      expect(terminalOutputFlushDeferredForTesting(session), isFalse);

      flushTerminalOutputForTesting(session);

      // Output arriving right behind a flush waits out the floor rather than
      // driving another frame immediately.
      queueTerminalOutputForTesting(session, 'second\r\n');
      expect(terminalOutputFlushDeferredForTesting(session), isTrue);

      await Future.pause(terminalOutputMinFlushIntervalForTesting * 2);

      expect(terminalOutputFlushDeferredForTesting(session), isFalse);
      expect(terminalOutputFlushScheduledForTesting(session), isTrue);
    },
  );

  test('a deferred flush is dropped when the terminal goes hidden', () async {
    final runtime = XtermTerminalRuntime(
      ptySessionFactory: _FakeTerminalPtySessionFactory(),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        _launch('shell', shell: '/bin/sh'),
      ],
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    final visibility = acquireTerminalVisibilityForTesting(session);

    queueTerminalOutputForTesting(session, 'first\r\n');
    flushTerminalOutputForTesting(session);
    queueTerminalOutputForTesting(session, 'second\r\n');
    expect(terminalOutputFlushDeferredForTesting(session), isTrue);

    visibility.dispose();
    await Future.pause(terminalOutputMinFlushIntervalForTesting * 2);

    // The backlog is kept, but a hidden terminal must not pay frame time for
    // it; it drains when it becomes visible again.
    expect(terminalOutputFlushScheduledForTesting(session), isFalse);
    expect(pendingTerminalOutputCharsForTesting(session), greaterThan(0));
  });
}
