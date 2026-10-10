part of 'terminal_runtime.dart';

/// Recovery for a parser-worker replica that stopped matching its worker.
extension _XtermTerminalParserWorkerResync on _XtermTerminalSessionHandle {
  /// Applies an incremental delta, falling back to a full snapshot when the
  /// replica no longer lines up with the worker.
  ///
  /// A rejected delta leaves the replica partly updated, and every later
  /// incremental delta would build on that, so the tab stayed garbled until it
  /// was evicted. The snapshot runs next on the same command tail, ahead of
  /// any output queued after this delta.
  void _applyParserWorkerBufferDelta(
    TerminalXtermReplicaTerminal terminal,
    int generation,
    TerminalXtermWorkerBufferDelta delta,
  ) {
    try {
      terminal.applyBufferDelta(delta);
    } on StateError catch (error, stackTrace) {
      FlutterError.reportError(
        FlutterErrorDetails(
          exception: error,
          stack: stackTrace,
          library: 'terminal runtime',
          context: ErrorDescription(
            'applying terminal parser worker delta; resynchronizing replica',
          ),
        ),
      );
      final resync = _queueParserWorkerCommand(terminal, generation, (
        worker,
      ) async {
        final snapshot = await worker.snapshotBufferDelta();
        if (_disposed ||
            generation != _parserWorkerGeneration ||
            !identical(_terminal, terminal)) {
          return;
        }
        _applyParserWorkerEffects(snapshot.effects);
        terminal.applyBufferDelta(snapshot);
      });
      _parserWorkerLastApply = resync;
      unawaited(
        resync.catchError((Object error, StackTrace stackTrace) {
          if (_disposed || generation != _parserWorkerGeneration) {
            return;
          }
          FlutterError.reportError(
            FlutterErrorDetails(
              exception: error,
              stack: stackTrace,
              library: 'terminal runtime',
              context: ErrorDescription('resynchronizing terminal replica'),
            ),
          );
        }),
      );
    }
  }
}
