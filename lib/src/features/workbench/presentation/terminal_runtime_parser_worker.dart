part of 'terminal_runtime.dart';

/// Parser/model isolation for the opt-in xterm worker backend.
extension _XtermTerminalParserWorker on _XtermTerminalSessionHandle {
  void _resetParserWorkerBackend() {
    if (!_parserWorkerEnabled) {
      return;
    }
    _parserWorkerGeneration += 1;
    final workerFuture = _parserWorkerFuture;
    final commandTail = _parserWorkerCommandTail;
    _parserWorkerFuture = null;
    _parserWorkerCommandTail = Future<void>.value();
    _parserWorkerLastApply = null;
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
    final workerFuture = TerminalXtermWorker.start(
      cols: terminal.viewWidth,
      rows: terminal.viewHeight,
      maxLines: _settings.scrollbackLines,
      platform: _xtermTargetPlatform,
      wordSeparators: _rendererAdapterOwner.resolveWordSeparators(
        _settings.wordSeparators,
      ),
    );
    final validatedFuture = workerFuture.then((worker) async {
      if (_disposed ||
          generation != _parserWorkerGeneration ||
          !identical(_terminal, terminal)) {
        await worker.close();
        throw StateError('Terminal parser worker generation was replaced.');
      }
      return worker;
    });
    _parserWorkerFuture = validatedFuture;
    return validatedFuture;
  }

  Future<void> _writeToParserWorker(String data) {
    final terminal = _terminal;
    if (terminal is! TerminalXtermReplicaTerminal) {
      throw StateError('Parser worker backend requires an xterm replica.');
    }
    final generation = _parserWorkerGeneration;
    final command = _queueParserWorkerCommand(terminal, generation, (
      worker,
    ) async {
      final delta = await worker.writeBufferDelta(
        data,
        focused: _parserWorkerFocused,
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
    final previous = _parserWorkerCommandTail;
    final next = previous.then<void>(
      (_) async {
        final worker = await _ensureParserWorker(terminal, generation);
        await command(worker);
      },
      onError: (Object _, StackTrace __) async {
        final worker = await _ensureParserWorker(terminal, generation);
        await command(worker);
      },
    );
    _parserWorkerCommandTail = next.then<void>(
      (_) {},
      onError: (Object _, StackTrace __) {},
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
