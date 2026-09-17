part of 'terminal_runtime.dart';

/// Callbacks the handle supplies so [_TerminalSessionOutputPump] can write
/// terminal data and advance synchronization state without a back-reference
/// to the full handle.
abstract interface class _TerminalSessionOutputHost {
  bool get isDisposed;
  bool get isOutputVisible;

  /// Applies one ordered output chunk.
  ///
  /// Returning null means the chunk was applied synchronously. A future keeps
  /// this pump from dispatching another chunk until the asynchronous emulator
  /// update has completed.
  Future<void>? writeToTerminal(String data);
  void advanceRestore(int chars);
  void advancePointerInputCatchUp(int chars);
}

/// Owns the output pipeline and drives all queuing, overflow protection,
/// scheduling, and draining for one terminal session.
///
/// Extracted from the free functions in terminal_runtime_output_batching.dart
/// so the handle no longer owns raw pipeline state or batching policy.
class _TerminalSessionOutputPump {
  _TerminalSessionOutputPump(this._host);

  final _TerminalSessionOutputHost _host;
  final _TerminalOutputPipeline pipeline = _TerminalOutputPipeline();
  bool _hiddenCatchUpScheduled = false;
  bool _writeInFlight = false;
  int _writeGeneration = 0;
  // Start conservatively before this session has any parse-time samples.
  // Local profiling of ANSI/TUI-heavy output measured ~25 ms at 64 KiB,
  // ~13 ms at 32 KiB, and ~6.7 ms at 16 KiB. Plain/normal ANSI output is
  // cheap and will quickly grow back toward the 64 KiB ceiling.
  int _adaptiveChunkBudget = _terminalOutputInitialAdaptiveCharsPerFrame;

  void capAdaptiveBudgetForReveal() {
    if (_adaptiveChunkBudget > _terminalOutputInitialAdaptiveCharsPerFrame) {
      _adaptiveChunkBudget = _terminalOutputInitialAdaptiveCharsPerFrame;
    }
  }

  void queue(
    String data, {
    _TerminalOutputSource source = _TerminalOutputSource.live,
  }) {
    if (data.isEmpty || _host.isDisposed) {
      return;
    }
    if (source == _TerminalOutputSource.live &&
        pipeline.pending.isEmpty &&
        pipeline.sinceFlushRequest.isRunning &&
        pipeline.sinceFlushRequest.elapsed >=
            _terminalOutputAdaptiveIdleResetInterval) {
      // A learned 64 KiB budget is safe only while the workload stays similar.
      // After a quiet gap the next burst may be a full-screen TUI, where local
      // profiling measured a ~25 ms first parse at 64 KiB versus ~6-7 ms at
      // 16 KiB. Start the new burst conservatively and learn upward again.
      capAdaptiveBudgetForReveal();
    }
    pipeline.add(_TerminalOutputSegment(data, source));
    if (source == _TerminalOutputSource.live &&
        pipeline.liveLength > _terminalOutputMaxPendingChars) {
      if (_host.isOutputVisible) {
        // A visible burst should not pay a parse budget inside every host
        // output callback. Promote normal overflow to the next frame instead;
        // this bounds parser work to the render cadence while keeping the
        // application responsive. Only the much higher hard-water mark keeps
        // one bounded synchronous drain as a local-memory safety valve.
        if (pipeline.liveLength > _terminalOutputVisibleHardPendingChars) {
          _drainChunk(adaptBudget: true);
        }
        _scheduleUrgentVisibleFlush();
        return;
      }
      // Never discard terminal output just to protect the UI queue, but also
      // never parse a full 1 MiB backlog synchronously on the Flutter UI
      // isolate. Restore, control, and live segments stay ordered in the same
      // queue, so a partial snapshot can safely continue across these chunks;
      // its overlay and pointer suspension are released only as restore bytes
      // are actually consumed. Consume one adaptive budget now, then yield
      // between additional hidden catch-up chunks.
      _drainChunk(adaptBudget: true);
      if (!_host.isOutputVisible &&
          pipeline.liveLength > _terminalOutputHiddenCatchUpTargetChars) {
        _scheduleHiddenCatchUp();
      } else {
        scheduleFlush();
      }
      return;
    }
    scheduleFlush();
  }

  void _scheduleUrgentVisibleFlush() {
    if (_writeInFlight || _host.isDisposed || !_host.isOutputVisible) {
      return;
    }
    // Replace a 50 ms cadence timer with the next vsync. A frame callback that
    // is already queued owns `flushScheduled` without a timer, so it is already
    // as urgent as we can make it and must not be duplicated.
    if (pipeline.flushTimer != null) {
      pipeline.cancelDeferredFlush();
    }
    if (pipeline.flushScheduled) {
      return;
    }
    _requestFrame();
  }

  void _scheduleHiddenCatchUp() {
    if (_writeInFlight ||
        _hiddenCatchUpScheduled ||
        _host.isDisposed ||
        _host.isOutputVisible) {
      return;
    }
    _hiddenCatchUpScheduled = true;
    Timer.run(() {
      _hiddenCatchUpScheduled = false;
      if (_host.isDisposed || _host.isOutputVisible) {
        return;
      }
      if (pipeline.liveLength <= _terminalOutputHiddenCatchUpTargetChars) {
        return;
      }
      _drainChunk(adaptBudget: true);
      if (pipeline.liveLength > _terminalOutputHiddenCatchUpTargetChars) {
        _scheduleHiddenCatchUp();
      }
    });
  }

  void scheduleFlush() {
    // A hidden terminal keeps a bounded accumulation window without paying
    // frame time for it. Once live output crosses the 1 MiB high-water mark,
    // queue() yields through catch-up chunks down to a smaller low-water mark;
    // the final partial window is drained when the terminal becomes visible.
    if (_writeInFlight ||
        pipeline.flushScheduled ||
        _host.isDisposed ||
        !_host.isOutputVisible) {
      return;
    }
    final clock = pipeline.sinceFlushRequest;
    // An unstarted clock means nothing has been flushed yet, so the first
    // chunk goes out on the next frame rather than waiting for a cadence it
    // has not used up.
    final sinceLastFlush = clock.isRunning
        ? clock.elapsed
        : _terminalOutputMinFlushInterval;
    if (sinceLastFlush >= _terminalOutputMinFlushInterval) {
      _requestFrame();
      return;
    }
    pipeline.flushScheduled = true;
    pipeline.flushTimer = Timer(
      _terminalOutputMinFlushInterval - sinceLastFlush,
      () {
        pipeline.flushTimer = null;
        pipeline.flushScheduled = false;
        if (_host.isDisposed || !_host.isOutputVisible) {
          return;
        }
        _requestFrame();
      },
    );
  }

  void _requestFrame() {
    pipeline.flushScheduled = true;
    // The cadence is measured from here rather than from the flush that
    // follows, because the frame callback lands up to a vsync later: charging
    // that wait to the next interval as well would pace the terminal at 20 Hz,
    // not 30.
    pipeline.restartFlushClock();
    SchedulerBinding.instance.scheduleFrameCallback((_) {
      flushFrame();
    });
    SchedulerBinding.instance.ensureVisualUpdate();
  }

  void flushFrame({bool force = false}) {
    pipeline.flushScheduled = false;
    pipeline.flushCount += 1;
    if (_host.isDisposed) {
      clearPending();
      return;
    }
    if (!force && !_host.isOutputVisible) {
      return;
    }
    if (_writeInFlight) {
      return;
    }
    _drainChunk(adaptBudget: !force);
    if (pipeline.pending.isNotEmpty) {
      if (!force &&
          _host.isOutputVisible &&
          pipeline.liveLength > _terminalOutputMaxPendingChars) {
        _scheduleUrgentVisibleFlush();
      } else {
        scheduleFlush();
      }
    }
  }

  /// Writes at most one frame's worth of pending output, consuming the chunk
  /// that straddles the budget in place so the rest is never copied.
  void _drainChunk({bool adaptBudget = false}) {
    if (_writeInFlight) {
      return;
    }
    final pending = pipeline.pending;
    if (pending.isEmpty) {
      return;
    }
    final chunkBudget = _adaptiveChunkBudget;
    final frame = StringBuffer();
    var written = 0;
    var restoreWritten = 0;
    while (pending.isNotEmpty && written < chunkBudget) {
      final segment = pending.first;
      final head = segment.head;
      final available = segment.remaining;
      final remaining = chunkBudget - written;
      if (available <= remaining) {
        frame.write(segment.remainingText);
        pipeline.consume(segment, available);
        written += available;
        if (segment.source == _TerminalOutputSource.restore) {
          restoreWritten += available;
        }
        continue;
      }
      // Absolute index, because the budget is measured from the head, not
      // from the start of the chunk.
      final cutoff = _terminalOutputChunkCutoff(segment.text, head + remaining);
      if (cutoff <= head) {
        break;
      }
      final consumed = cutoff - head;
      frame.write(segment.text.substring(head, cutoff));
      pipeline.consume(segment, consumed);
      written += consumed;
      if (segment.source == _TerminalOutputSource.restore) {
        restoreWritten += consumed;
      }
    }
    if (written == 0) {
      return;
    }
    final shouldAdapt = adaptBudget && written >= chunkBudget - 1;
    final parseClock = shouldAdapt ? (Stopwatch()..start()) : null;
    final generation = _writeGeneration;
    final writeFuture = _host.writeToTerminal(frame.toString());
    if (writeFuture == null) {
      _completeWrite(
        generation: generation,
        written: written,
        restoreWritten: restoreWritten,
        chunkBudget: chunkBudget,
        parseClock: parseClock,
      );
      return;
    }

    _writeInFlight = true;
    unawaited(
      writeFuture.then(
        (_) => _completeWrite(
          generation: generation,
          written: written,
          restoreWritten: restoreWritten,
          chunkBudget: chunkBudget,
          parseClock: parseClock,
        ),
        onError: (Object error, StackTrace stackTrace) {
          _writeInFlight = false;
          parseClock?.stop();
          FlutterError.reportError(
            FlutterErrorDetails(
              exception: error,
              stack: stackTrace,
              library: 'terminal runtime',
              context: ErrorDescription('applying terminal output'),
            ),
          );
          _scheduleAfterWrite();
        },
      ),
    );
  }

  void _completeWrite({
    required int generation,
    required int written,
    required int restoreWritten,
    required int chunkBudget,
    required Stopwatch? parseClock,
  }) {
    _writeInFlight = false;
    if (parseClock != null) {
      parseClock.stop();
    }
    if (generation == _writeGeneration) {
      if (parseClock != null) {
        _adaptiveChunkBudget = _terminalOutputNextAdaptiveChunkBudget(
          currentChars: chunkBudget,
          parseTime: parseClock.elapsed,
        );
      }
      _host.advanceRestore(restoreWritten);
      _host.advancePointerInputCatchUp(written);
    }
    _scheduleAfterWrite();
  }

  void _scheduleAfterWrite() {
    if (_host.isDisposed || pipeline.pending.isEmpty) {
      return;
    }
    if (_host.isOutputVisible) {
      scheduleFlush();
      return;
    }
    if (pipeline.liveLength > _terminalOutputHiddenCatchUpTargetChars) {
      _scheduleHiddenCatchUp();
    }
  }

  void clearPending() {
    _writeGeneration += 1;
    _hiddenCatchUpScheduled = false;
    pipeline.cancelDeferredFlush();
    pipeline.clear();
  }
}
