part of 'terminal_runtime_native_test.dart';

String _terminalRestorePayload(int length, String suffix) {
  const control = '\x1b[0m';
  final bodyLength = length - suffix.length;
  assert(bodyLength >= 0);
  final controls = List<String>.filled(
    bodyLength ~/ control.length,
    control,
  ).join();
  final padding = List<String>.filled(bodyLength - controls.length, ' ').join();
  return '$controls$padding$suffix';
}

void _registerTerminalRuntimeSnapshotTests() {
  test(
    'parser worker snapshot rebuild starts from a fresh generation',
    () async {
      final fakeSession = _FakeTerminalPtySession();
      final runtime = XtermTerminalRuntime(
        parserWorkerEnabled: true,
        ptySessionFactory: _FakeTerminalPtySessionFactory(
          sessions: <_FakeTerminalPtySession>[fakeSession],
        ),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      final visibility = acquireTerminalVisibilityForTesting(session);
      addTearDown(visibility.dispose);
      await session.ensureStarted();
      expect(terminalSearchUsesReplicaModelForTesting(session), isTrue);
      final searchController = session.searchController!
        ..open()
        ..setQuery('old-marker');

      queueTerminalOutputForTesting(session, 'old-marker\x1b[?2004h');
      flushTerminalOutputForTesting(session);
      await waitForTerminalParserApplyForTesting(session);
      expect(terminalBufferTextForTesting(session), contains('old-marker'));
      expect(terminalBracketedPasteModeForTesting(session), isTrue);
      expect(searchController.matchCount, 1);

      rebuildTerminalFromSnapshotTextForTesting(session, 'fresh-snapshot');
      expect(terminalSearchUsesReplicaModelForTesting(session), isTrue);
      flushTerminalOutputForTesting(session);
      await waitForTerminalParserApplyForTesting(session);

      expect(searchController.matchCount, 0);
      searchController.setQuery('fresh-snapshot');
      expect(searchController.matchCount, 1);
      final restored = terminalBufferTextForTesting(session);
      expect(restored, contains('fresh-snapshot'));
      expect(restored, isNot(contains('old-marker')));
      expect(terminalBracketedPasteModeForTesting(session), isFalse);

      queueTerminalOutputForTesting(session, '\r\nlive-after-snapshot');
      flushTerminalOutputForTesting(session);
      await waitForTerminalParserApplyForTesting(session);
      expect(
        terminalBufferTextForTesting(session),
        contains('live-after-snapshot'),
      );
    },
  );

  test(
    'parser worker snapshot hydrates directly and keeps live output ordered',
    () async {
      final fakeSession = _FakeTerminalPtySession();
      final runtime = XtermTerminalRuntime(
        parserWorkerEnabled: true,
        ptySessionFactory: _FakeTerminalPtySessionFactory(
          sessions: <_FakeTerminalPtySession>[fakeSession],
        ),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      final visibility = acquireTerminalVisibilityForTesting(session);
      addTearDown(visibility.dispose);
      await session.ensureStarted();

      const snapshotMarker = 't8d-snapshot-marker';
      const liveMarker = 't8d-live-marker';
      final snapshot = _terminalRestorePayload(
        512 * 1024,
        '\r\n$snapshotMarker\r\n\x1b[?2004h\x1b[?1000h',
      );

      rebuildTerminalFromSnapshotTextForTesting(session, snapshot);
      expect(pendingRestoreTerminalOutputCharsForTesting(session), 0);
      await waitForTerminalParserApplyForTesting(session);
      expect(session.restoreProgress.value, isNull);
      expect(terminalPointerInputSuspendedForTesting(session), isFalse);
      expect(terminalBufferTextForTesting(session), contains(snapshotMarker));

      queueTerminalOutputForTesting(session, '\r\n$liveMarker');
      flushTerminalOutputForTesting(session);
      await waitForTerminalParserApplyForTesting(session);

      final text = terminalBufferTextForTesting(session);
      expect(text, contains(snapshotMarker));
      expect(text, contains(liveMarker));
      expect(text.indexOf(snapshotMarker), lessThan(text.indexOf(liveMarker)));
    },
  );

  test(
    'snapshot replacement invalidates stale in-flight output writes',
    () async {
      final fakeSession = _FakeTerminalPtySession();
      final runtime = XtermTerminalRuntime(
        parserWorkerEnabled: true,
        ptySessionFactory: _FakeTerminalPtySessionFactory(
          sessions: <_FakeTerminalPtySession>[fakeSession],
        ),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      final visibility = acquireTerminalVisibilityForTesting(session);
      addTearDown(visibility.dispose);
      await session.ensureStarted();

      queueTerminalOutputForTesting(session, 'worker-warmup');
      flushTerminalOutputForTesting(session);
      await waitForTerminalParserApplyForTesting(session);

      queueTerminalOutputForTesting(session, '\x1b[0m' * (128 * 1024));
      flushTerminalOutputForTesting(session);
      expect(terminalOutputWriteInFlightForTesting(session), isTrue);

      rebuildTerminalFromSnapshotTextForTesting(
        session,
        '\r\nt8d-replacement-snapshot\r\n',
      );
      expect(terminalOutputWriteInFlightForTesting(session), isFalse);

      queueTerminalOutputForTesting(session, '\r\nt8d-new-generation-live');
      flushTerminalOutputForTesting(session);
      await waitForTerminalParserApplyForTesting(session);

      final text = terminalBufferTextForTesting(session);
      expect(text, contains('t8d-replacement-snapshot'));
      expect(text, contains('t8d-new-generation-live'));
    },
  );

  test(
    'parser worker direct snapshot reset clears interaction modes',
    () async {
      final fakeSession = _FakeTerminalPtySession();
      final runtime = XtermTerminalRuntime(
        parserWorkerEnabled: true,
        ptySessionFactory: _FakeTerminalPtySessionFactory(
          sessions: <_FakeTerminalPtySession>[fakeSession],
        ),
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
          _launch('shell', shell: '/bin/sh'),
        ],
      );
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      final visibility = acquireTerminalVisibilityForTesting(session);
      addTearDown(visibility.dispose);
      await session.ensureStarted();

      const marker = 't8d-reset-marker';
      final snapshot = _terminalRestorePayload(
        512 * 1024,
        '\r\n$marker\r\n\x1b[?2004h\x1b[?1000h',
      );

      rebuildTerminalFromSnapshotTextForTesting(
        session,
        snapshot,
        resetInteractionModes: true,
      );
      expect(pendingRestoreTerminalOutputCharsForTesting(session), 0);
      await waitForTerminalParserApplyForTesting(session);

      expect(session.restoreProgress.value, isNull);
      expect(terminalPointerInputSuspendedForTesting(session), isFalse);
      expect(terminalBracketedPasteModeForTesting(session), isFalse);
      expect(terminalMouseModeForTesting(session), xterm.MouseMode.none);
      expect(terminalBufferTextForTesting(session), contains(marker));
    },
  );

  test(
    'pauses output while the app is backgrounded and restores on foreground',
    () async {
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
      addTearDown(runtime.dispose);
      final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
      TerminalVisibilityLease? visibility;
      try {
        visibility = acquireTerminalVisibilityForTesting(session);
        await session.ensureStarted();

        fakeSession.emitOutput(utf8.encode('visible\r\n'));
        await Future.pause(.zero);
        flushTerminalOutputForTesting(session);
        expect(terminalBufferTextForTesting(session), contains('visible'));
        runtime.setAppForeground(false);
        await Future.pause(.zero);
        expect(fakeSession.outputPausedCalls, contains(true));
        fakeSession.emitOutput(utf8.encode('hidden\r\n'));
        await Future.pause(.zero);
        expect(
          terminalBufferTextForTesting(session),
          isNot(contains('hidden')),
        );
        runtime.setAppForeground(true);
        await Future.pause(.zero);
        expect(fakeSession.outputPausedCalls.last, isFalse);
        fakeSession.emitSnapshot(
          utf8.encode(
            '\x1b[?1h\x1b[?25l\x1b[?1004h\x1b=\x1b[?1000h\x1b[?2004h\x1b[?1049hvisible\r\nhidden\r\n',
          ),
        );
        await Future.pause(.zero);
        flushTerminalOutputForTesting(session);
        final restored = terminalBufferTextForTesting(session);
        expect(restored, contains('visible'));
        expect(restored, contains('hidden'));
        expect(
          terminalMouseModeForTesting(session),
          isNot(xterm.MouseMode.none),
        );
        expect(terminalBracketedPasteModeForTesting(session), isTrue);
        expect(terminalCursorKeysModeForTesting(session), isTrue);
        expect(terminalCursorVisibleModeForTesting(session), isFalse);
        expect(terminalReportFocusModeForTesting(session), isTrue);
        expect(terminalAppKeypadModeForTesting(session), isTrue);
        expect(terminalIsUsingAltBufferForTesting(session), isTrue);
      } finally {
        visibility?.dispose();
        debugDefaultTargetPlatformOverride = null;
      }
    },
  );

  test('a process exit mid-restore takes the restore overlay down', () async {
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
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    TerminalVisibilityLease? visibility;
    try {
      visibility = acquireTerminalVisibilityForTesting(session);
      await session.ensureStarted();

      // Three frames' worth, so one drain cannot finish the restore.
      fakeSession.emitSnapshot(utf8.encode('a' * (64 * 1024 * 3)));
      await Future.pause(.zero);
      flushTerminalOutputForTesting(session);
      expect(session.restoreProgress.value, isNotNull);

      fakeSession.emitExit(0);
      await Future.pause(.zero);

      expect(session.restoreProgress.value, isNull);
    } finally {
      visibility?.dispose();
      debugDefaultTargetPlatformOverride = null;
    }
  });

  test('a snapshot keeps pointer input suspended until it is parsed', () async {
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
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    final visibility = acquireTerminalVisibilityForTesting(session);
    try {
      await session.ensureStarted();

      fakeSession.emitSnapshot(utf8.encode('\x1b[?1000h\x1b[?1006hactive tui'));
      await Future.pause(.zero);

      expect(terminalPointerInputSuspendedForTesting(session), isTrue);

      flushTerminalOutputForTesting(session);

      expect(terminalPointerInputSuspendedForTesting(session), isFalse);
      expect(terminalMouseModeForTesting(session), isNot(xterm.MouseMode.none));
    } finally {
      visibility.dispose();
      debugDefaultTargetPlatformOverride = null;
    }
  });

  test('a predecoded snapshot restores without going through bytes', () async {
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
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    final visibility = acquireTerminalVisibilityForTesting(session);
    try {
      await session.ensureStarted();

      fakeSession.emitSnapshotText(
        '\x1b[?1000h\x1b[?1006hpredecoded ñ snapshot',
      );
      await Future.pause(.zero);

      expect(terminalPointerInputSuspendedForTesting(session), isTrue);
      flushTerminalOutputForTesting(session);

      expect(terminalPointerInputSuspendedForTesting(session), isFalse);
      expect(
        terminalBufferTextForTesting(session),
        contains('predecoded ñ snapshot'),
      );
      expect(terminalMouseModeForTesting(session), isNot(xterm.MouseMode.none));
    } finally {
      visibility.dispose();
      debugDefaultTargetPlatformOverride = null;
    }
  });

  test('live output cannot evict a restore before its first flush', () async {
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
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    final visibility = acquireTerminalVisibilityForTesting(session);
    try {
      await session.ensureStarted();
      const snapshotLength = 64 * 1024 * 18;
      const snapshotMarker = 'snapshot-tail-marker';
      const liveMarker = 'live-tail-marker';
      final snapshot = _terminalRestorePayload(
        snapshotLength,
        '\r\n$snapshotMarker\r\n',
      );

      fakeSession.emitSnapshot(utf8.encode(snapshot));
      await Future.pause(.zero);
      fakeSession.emitOutput(utf8.encode('$liveMarker\r\n'));
      await Future.pause(.zero);

      expect(
        pendingRestoreTerminalOutputCharsForTesting(session),
        snapshotLength,
      );
      expect(
        pendingLiveTerminalOutputCharsForTesting(session),
        '$liveMarker\r\n'.length,
      );
      expect(session.restoreProgress.value?.totalChars, snapshotLength);

      while (pendingRestoreTerminalOutputCharsForTesting(session) > 0) {
        flushTerminalOutputForTesting(session);
        final remaining = pendingRestoreTerminalOutputCharsForTesting(session);
        final progress = session.restoreProgress.value;
        if (remaining == 0) {
          expect(progress, isNull);
        } else {
          expect(progress, isNotNull);
          expect(progress!.writtenChars + remaining, snapshotLength);
        }
      }

      expect(
        pendingLiveTerminalOutputCharsForTesting(session),
        '$liveMarker\r\n'.length,
      );
      while (pendingTerminalOutputCharsForTesting(session) > 0) {
        flushTerminalOutputForTesting(session);
      }
      final text = terminalBufferTextForTesting(session);
      expect(text, contains(snapshotMarker));
      expect(text, contains(liveMarker));
      expect(text.indexOf(snapshotMarker), lessThan(text.indexOf(liveMarker)));
    } finally {
      visibility.dispose();
      debugDefaultTargetPlatformOverride = null;
    }
  });

  test('overflow during a partial restore stays budgeted without dropping live output', () async {
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
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    final visibility = acquireTerminalVisibilityForTesting(session);
    try {
      await session.ensureStarted();
      const snapshotLength = 64 * 1024 * 18;
      const snapshotMarker = 'protected-snapshot-marker';
      const oldLiveMarker = 'preserved-live-marker';
      const newLiveMarker = 'retained-live-marker';
      final snapshot = _terminalRestorePayload(
        snapshotLength,
        '\r\n$snapshotMarker\r\n\x1b[?1000h\x1b[?2004h',
      );

      fakeSession.emitSnapshot(
        utf8.encode(snapshot),
        resetInteractionModes: true,
      );
      await Future.pause(.zero);
      flushTerminalOutputForTesting(session);
      final restoreAfterFirstFlush =
          pendingRestoreTerminalOutputCharsForTesting(session);

      // The reset queued by the snapshot turns bracketed paste off. If the
      // old live prefix were trimmed, this mode would stay off after the
      // overflow flush, so the assertion below proves the prefix was parsed.
      const oldLivePrefix = '\x1b[?2004h$oldLiveMarker\r\n';
      final oldLive =
          oldLivePrefix +
          _terminalRestorePayload(128 * 1024 - oldLivePrefix.length, '');
      final newLive = _terminalRestorePayload(
        1200 * 1024,
        '$newLiveMarker\r\n',
      );
      fakeSession.emitOutput(utf8.encode(oldLive));
      fakeSession.emitOutput(utf8.encode(newLive));
      await Future.pause(.zero);

      expect(restoreAfterFirstFlush, greaterThan(0));
      expect(
        pendingRestoreTerminalOutputCharsForTesting(session),
        greaterThan(0),
      );
      expect(pendingLiveTerminalOutputCharsForTesting(session), greaterThan(0));
      expect(session.restoreProgress.value, isNotNull);
      expect(terminalPointerInputSuspendedForTesting(session), isTrue);

      while (pendingTerminalOutputCharsForTesting(session) > 0) {
        flushTerminalOutputForTesting(session);
      }

      expect(pendingRestoreTerminalOutputCharsForTesting(session), 0);
      expect(pendingLiveTerminalOutputCharsForTesting(session), 0);
      expect(session.restoreProgress.value, isNull);
      expect(terminalPointerInputSuspendedForTesting(session), isFalse);
      expect(terminalBracketedPasteModeForTesting(session), isTrue);

      final text = terminalBufferTextForTesting(session);
      // The snapshot was parsed before the live bytes, but xterm's own
      // bounded scrollback may naturally roll its old visible marker away
      // after this deliberately huge write. The mode assertion above proves
      // the formerly-trimmed live prefix itself was not dropped.
      expect(text, contains(newLiveMarker));
      expect(terminalMouseModeForTesting(session), xterm.MouseMode.none);
    } finally {
      visibility.dispose();
      debugDefaultTargetPlatformOverride = null;
    }
  });

  testWidgets('a snapshot reschedules the deferred flush it replaces', (
    tester,
  ) async {
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
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    final visibility = acquireTerminalVisibilityForTesting(session);
    try {
      await session.ensureStarted();
      queueTerminalOutputForTesting(session, 'first');
      await tester.pump();
      expect(pendingTerminalOutputCharsForTesting(session), 0);
      queueTerminalOutputForTesting(session, 'pending before replacement');
      forceDeferredTerminalOutputFlushForTesting(session);
      expect(terminalOutputFlushDeferredForTesting(session), isTrue);

      const replacement = 'replacement snapshot';
      fakeSession.emitSnapshot(utf8.encode(replacement));
      await tester.pump(terminalOutputMinFlushIntervalForTesting * 2);

      expect(pendingRestoreTerminalOutputCharsForTesting(session), 0);
      expect(session.restoreProgress.value, isNull);
      expect(terminalBufferTextForTesting(session), contains(replacement));
    } finally {
      visibility.dispose();
      debugDefaultTargetPlatformOverride = null;
    }
  });
}
