part of 'terminal_runtime.dart';

/// Parser/model isolation for the opt-in xterm worker backend.
extension _XtermTerminalParserWorker on _XtermTerminalSessionHandle {
  bool _hydrateParserWorkerSnapshot(
    String restored, {
    required bool resetInteractionModes,
  }) {
    if (!_parserWorkerEnabled || _disposed) {
      return false;
    }
    final terminal = _terminal;
    if (terminal is! TerminalXtermReplicaTerminal) {
      return false;
    }
    final generation = _parserWorkerGeneration;
    _lastSnapshotHydrationProfile = null;
    _beginRestore(restored.length);
    final command = _snapshotHydrationProfilingEnabled
        ? _queueParserWorkerCommand(terminal, generation, (worker) async {
            final hydrationWatch = Stopwatch()..start();
            final queueAndStartupMicros = hydrationWatch.elapsedMicroseconds;
            final profile = await worker.profileHydrateSnapshotBufferDelta(
              restored,
              resetInteractionModes: resetInteractionModes,
            );
            if (_disposed ||
                generation != _parserWorkerGeneration ||
                !identical(_terminal, terminal)) {
              return;
            }
            final applyWatch = Stopwatch()..start();
            _applyParserWorkerEffects(profile.delta.effects);
            terminal.applyBufferDelta(profile.delta);
            applyWatch.stop();
            hydrationWatch.stop();
            _lastSnapshotHydrationProfile = (
              snapshotRevision: profile.delta.revision,
              queueAndStartupMicros: queueAndStartupMicros,
              workerParseMicros: profile.parseMicros,
              workerMaterializeMicros: profile.materializeMicros,
              workerRoundtripMicros: profile.rawRoundtripMicros,
              decodeMicros: profile.decodeMicros,
              uiApplyMicros: applyWatch.elapsedMicroseconds,
              totalHydrationMicros: hydrationWatch.elapsedMicroseconds,
            );
            _completeRestoreProgress();
            _completePointerInputDirectSnapshotHydration();
          })
        : _queueParserWorkerCommand(terminal, generation, (worker) async {
            final delta = await worker.hydrateSnapshotBufferDelta(
              restored,
              resetInteractionModes: resetInteractionModes,
            );
            if (_disposed ||
                generation != _parserWorkerGeneration ||
                !identical(_terminal, terminal)) {
              return;
            }
            _applyParserWorkerEffects(delta.effects);
            terminal.applyBufferDelta(delta);
            _completeRestoreProgress();
            _completePointerInputDirectSnapshotHydration();
          });
    _parserWorkerLastApply = command;
    unawaited(
      command.catchError((Object error, StackTrace stackTrace) {
        if (_disposed || generation != _parserWorkerGeneration) {
          return;
        }
        _finishRestore();
        _completePointerInputDirectSnapshotHydration();
        FlutterError.reportError(
          FlutterErrorDetails(
            exception: error,
            stack: stackTrace,
            library: 'terminal runtime',
            context: ErrorDescription('hydrating terminal snapshot'),
          ),
        );
      }),
    );
    return true;
  }

  void _resetParserWorkerBackend() {
    if (!_parserWorkerEnabled) {
      return;
    }
    _parserWorkerGeneration += 1;
    _parserWorkerReady = false;
    _uiBufferEvicted = false;
    final workerFuture = _parserWorkerFuture;
    final commandTail = _parserWorkerCommandTail;
    _parserWorkerFuture = null;
    _parserWorkerRetainedState = null;
    _parserWorkerHardEvictionBlockers = const <String>[];
    _parserWorkerCommandTail = Future<void>.value();
    _parserWorkerLastApply = null;
    _parserWorkerReplicaNeedsSync = false;
    _pendingParserWorkerSize = null;
    if (workerFuture == null) {
      return;
    }

    Future<void> closeAfterDrain() async {
      try {
        await commandTail;
      } catch (_) {
        // A stale generation can reject while being replaced. The worker
        // startup validation or the close below owns cleanup in either case.
      }
      try {
        final worker = await workerFuture;
        await worker.close();
      } catch (_) {
        // A worker that loses the generation race closes itself before the
        // validated startup future completes, so there is nothing left to do.
      }
    }

    unawaited(closeAfterDrain());
  }

  Future<TerminalXtermWorker> _ensureParserWorker(
    TerminalXtermReplicaTerminal terminal,
    int generation,
  ) {
    final existing = _parserWorkerFuture;
    if (existing != null) {
      return existing;
    }
    final retainedState = _parserWorkerRetainedState;
    final workerFuture = TerminalXtermWorker.start(
      cols: terminal.viewWidth,
      rows: terminal.viewHeight,
      maxLines: _settings.scrollbackLines,
      platform: _xtermTargetPlatform,
      wordSeparators: _rendererAdapterOwner.resolveWordSeparators(
        _settings.wordSeparators,
      ),
      retainedState: retainedState,
    );
    final validatedFuture = workerFuture.then((worker) async {
      if (_disposed ||
          generation != _parserWorkerGeneration ||
          !identical(_terminal, terminal)) {
        await worker.close();
        throw StateError('Terminal parser worker generation was replaced.');
      }
      if (identical(_parserWorkerRetainedState, retainedState)) {
        _parserWorkerRetainedState = null;
      }
      _parserWorkerHardEvictionBlockers = const <String>[];
      _parserWorkerReady = true;
      return worker;
    });
    _parserWorkerFuture = validatedFuture;
    return validatedFuture;
  }

  Future<Duration> _writeToParserWorker(String data) {
    final terminal = _terminal;
    if (terminal is! TerminalXtermReplicaTerminal) {
      throw StateError('Parser worker backend requires an xterm replica.');
    }
    final generation = _parserWorkerGeneration;
    _flushPendingParserWorkerResize();
    final applyBuffer = _visibility.isOutputVisible;
    if (!applyBuffer) {
      // Mark dirty when the hidden command is queued, not when it completes.
      // A reveal that races the in-flight parse will therefore still enqueue a
      // snapshot behind it on the same worker command tail.
      _parserWorkerReplicaNeedsSync = true;
    }
    var uiApplyTime = Duration.zero;
    final command = _queueParserWorkerCommand(terminal, generation, (
      worker,
    ) async {
      if (applyBuffer) {
        final delta = await worker.writeBufferDelta(
          data,
          focused: _parserWorkerFocused,
        );
        if (_disposed ||
            generation != _parserWorkerGeneration ||
            !identical(_terminal, terminal)) {
          return;
        }
        final applyClock = Stopwatch()..start();
        _applyParserWorkerEffects(delta.effects);
        terminal.applyBufferDelta(delta);
        applyClock.stop();
        uiApplyTime = applyClock.elapsed;
        return;
      }

      final state = await worker.parseHidden(
        data,
        focused: _parserWorkerFocused,
      );
      if (_disposed ||
          generation != _parserWorkerGeneration ||
          !identical(_terminal, terminal)) {
        return;
      }
      final applyClock = Stopwatch()..start();
      _applyParserWorkerEffects(state.effects);
      terminal.applyStateDelta(state);
      applyClock.stop();
      uiApplyTime = applyClock.elapsed;
    });
    _parserWorkerLastApply = command;
    return command.then((_) => uiApplyTime);
  }

  bool _evictParserWorkerUiBuffer() {
    if (!_parserWorkerEnabled ||
        !_parserWorkerReady ||
        _disposed ||
        _outputVisible ||
        _uiBufferEvicted) {
      return false;
    }
    final previousTerminal = _terminal;
    if (previousTerminal is! TerminalXtermReplicaTerminal) {
      return false;
    }

    _terminalController.clearSelection();
    final viewWidth = previousTerminal.viewWidth;
    final viewHeight = previousTerminal.viewHeight;
    _detachTerminal(previousTerminal);

    final nextTerminal = _createTerminal(notificationsEnabled: false)
      ..resize(viewWidth, viewHeight);
    _terminal = nextTerminal;
    _attachTerminal(nextTerminal);
    _attachSearchTerminal(nextTerminal);
    _parserWorkerReplicaNeedsSync = true;
    _uiBufferEvicted = true;
    previousTerminal.dispose();
    _syncPtyOutputVisibility();
    _scheduleParserWorkerHardEviction(
      nextTerminal as TerminalXtermReplicaTerminal,
    );
    return true;
  }

  void _scheduleParserWorkerHardEviction(
    TerminalXtermReplicaTerminal terminal,
  ) {
    if (!_parserWorkerHardEvictionEnabledForTesting) {
      return;
    }
    final activeWorkerFuture = _parserWorkerFuture;
    if (activeWorkerFuture == null) {
      return;
    }
    final generation = _parserWorkerGeneration;
    final command = _queueParserWorkerCommand(terminal, generation, (
      worker,
    ) async {
      if (_disposed ||
          generation != _parserWorkerGeneration ||
          !identical(_terminal, terminal) ||
          !_uiBufferEvicted ||
          _outputVisible) {
        return;
      }
      final exported = await worker.exportRetainedState();
      if (_disposed ||
          generation != _parserWorkerGeneration ||
          !identical(_terminal, terminal) ||
          !_uiBufferEvicted ||
          _outputVisible) {
        return;
      }
      _parserWorkerHardEvictionBlockers = exported.blockers;
      final retainedState = exported.state;
      if (retainedState == null) {
        return;
      }

      _parserWorkerRetainedState = retainedState;
      _parserWorkerHardEvictionBlockers = const <String>[];
      _parserWorkerReady = false;
      try {
        await worker.close();
      } finally {
        if (generation == _parserWorkerGeneration &&
            identical(_parserWorkerFuture, activeWorkerFuture)) {
          _parserWorkerFuture = null;
        }
      }
    });
    _parserWorkerLastApply = command;
    unawaited(
      command.catchError((Object error, StackTrace stackTrace) {
        if (_disposed || generation != _parserWorkerGeneration) {
          return;
        }
        FlutterError.reportError(
          FlutterErrorDetails(
            exception: error,
            stack: stackTrace,
            library: 'terminal runtime',
            context: ErrorDescription('hard-evicting terminal parser worker'),
          ),
        );
      }),
    );
  }

  Future<void>? _syncParserWorkerReplicaForReveal() {
    if (!_parserWorkerEnabled || _disposed || !_parserWorkerReplicaNeedsSync) {
      return null;
    }
    final terminal = _terminal;
    if (terminal is! TerminalXtermReplicaTerminal) {
      return null;
    }
    final generation = _parserWorkerGeneration;
    _flushPendingParserWorkerResize();
    final command = _queueParserWorkerCommand(terminal, generation, (
      worker,
    ) async {
      final delta = await worker.snapshotBufferDelta();
      if (_disposed ||
          generation != _parserWorkerGeneration ||
          !identical(_terminal, terminal)) {
        return;
      }
      _applyParserWorkerEffects(delta.effects);
      terminal.applyBufferDelta(delta);
      _parserWorkerReplicaNeedsSync = false;
      _uiBufferEvicted = false;
    });
    _parserWorkerLastApply = command;
    unawaited(
      command.catchError((Object error, StackTrace stackTrace) {
        if (_disposed || generation != _parserWorkerGeneration) {
          return;
        }
        FlutterError.reportError(
          FlutterErrorDetails(
            exception: error,
            stack: stackTrace,
            library: 'terminal runtime',
            context: ErrorDescription('hydrating terminal parser worker'),
          ),
        );
      }),
    );
    return command;
  }

  void _resizeParserWorker({
    required int cols,
    required int rows,
    required int pixelWidth,
    required int pixelHeight,
  }) {
    if (!_parserWorkerEnabled || _disposed || _parserWorkerFuture == null) {
      return;
    }
    final terminal = _terminal;
    if (terminal is! TerminalXtermReplicaTerminal) {
      return;
    }
    final generation = _parserWorkerGeneration;
    final command = _queueParserWorkerCommand(terminal, generation, (
      worker,
    ) async {
      final delta = await worker.resizeBufferDelta(
        cols: cols,
        rows: rows,
        pixelWidth: pixelWidth,
        pixelHeight: pixelHeight,
      );
      if (_disposed ||
          generation != _parserWorkerGeneration ||
          !identical(_terminal, terminal)) {
        return;
      }
      _applyParserWorkerEffects(delta.effects);
      terminal.applyBufferDelta(delta);
    });
    _parserWorkerLastApply = command;
    unawaited(
      command.catchError((Object error, StackTrace stackTrace) {
        if (_disposed || generation != _parserWorkerGeneration) {
          return;
        }
        FlutterError.reportError(
          FlutterErrorDetails(
            exception: error,
            stack: stackTrace,
            library: 'terminal runtime',
            context: ErrorDescription('resizing terminal parser worker'),
          ),
        );
      }),
    );
  }

  Future<void> _queueParserWorkerCommand(
    TerminalXtermReplicaTerminal terminal,
    int generation,
    Future<void> Function(TerminalXtermWorker worker) command,
  ) {
    bool isCurrentGeneration() =>
        !_disposed &&
        generation == _parserWorkerGeneration &&
        identical(_terminal, terminal);

    Future<void> runCommand() async {
      if (!isCurrentGeneration()) {
        return;
      }
      try {
        final worker = await _ensureParserWorker(terminal, generation);
        if (!isCurrentGeneration()) {
          return;
        }
        await command(worker);
      } catch (_) {
        if (!isCurrentGeneration()) {
          return;
        }
        rethrow;
      }
    }

    final previous = _parserWorkerCommandTail;
    final next = previous.then<void>(
      (_) => runCommand(),
      onError: (Object _, StackTrace _) => runCommand(),
    );
    _parserWorkerCommandTail = next.then<void>(
      (_) {},
      onError: (Object _, StackTrace _) {},
    );
    return next;
  }

  void _applyParserWorkerEffects(List<TerminalXtermWorkerEffect> effects) {
    for (final effect in effects) {
      switch (effect) {
        case TerminalXtermWorkerTitleChanged(:final title):
          _handleTitleChanged(title);
        case TerminalXtermWorkerPtyWrite(:final data):
          _handleTerminalInput(data);
        case TerminalXtermWorkerClipboardStore(:final text):
          _storeTerminalClipboard(this, text);
        case TerminalXtermWorkerBell():
          break;
      }
    }
  }
}
