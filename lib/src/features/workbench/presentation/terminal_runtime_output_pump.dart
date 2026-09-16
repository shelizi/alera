part of 'terminal_runtime.dart';

/// Callbacks the handle supplies so [_TerminalSessionOutputPump] can write
/// terminal data and advance synchronization state without a back-reference
/// to the full handle.
abstract interface class _TerminalSessionOutputHost {
  bool get isDisposed;
  bool get isOutputVisible;

  void writeToTerminal(String data);
  void advanceRestore(int chars);
  void finishRestore();
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
  int _adaptiveChunkBudget = _terminalOutputMaxCharsPerFrame;

  void queue(
    String data, {
    _TerminalOutputSource source = _TerminalOutputSource.live,
  }) {
    if (data.isEmpty || _host.isDisposed) {
      return;
    }
    pipeline.add(_TerminalOutputSegment(data, source));
    if (source == _TerminalOutputSource.live &&
        pipeline.liveLength > _terminalOutputMaxPendingChars) {
      if (pipeline.restoreLength > 0) {
        // Snapshot restore is an atomic state transition: live bytes queued
        // behind it may depend on the restored cursor/mode state. Preserve the
        // existing correctness guarantee and finish that transition in order.
        flushNow();
        return;
      }
      // Never discard terminal output just to protect the UI queue, but also
      // never parse a full 1 MiB backlog synchronously on the Flutter UI
      // isolate. Consume one normal frame budget now so escape-sequence state
      // keeps advancing, then yield between additional hidden catch-up chunks.
      _drainChunk(adaptBudget: true);
      if (!_host.isOutputVisible &&
          pipeline.liveLength > _terminalOutputMaxPendingChars) {
        _scheduleHiddenCatchUp();
      } else {
        scheduleFlush();
      }
      return;
    }
    scheduleFlush();
  }

  void _scheduleHiddenCatchUp() {
    if (_hiddenCatchUpScheduled || _host.isDisposed || _host.isOutputVisible) {
      return;
    }
    _hiddenCatchUpScheduled = true;
    Timer.run(() {
      _hiddenCatchUpScheduled = false;
      if (_host.isDisposed || _host.isOutputVisible) {
        return;
      }
      if (pipeline.liveLength <= _terminalOutputMaxPendingChars) {
        return;
      }
      _drainChunk(adaptBudget: true);
      if (pipeline.liveLength > _terminalOutputMaxPendingChars) {
        _scheduleHiddenCatchUp();
      }
    });
  }

  void scheduleFlush() {
    // A hidden terminal keeps a bounded accumulation window without paying
    // frame time for it. Overflow is parsed directly by queue(), and the final
    // partial window is drained when the terminal becomes visible again.
    if (pipeline.flushScheduled || _host.isDisposed || !_host.isOutputVisible) {
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
    _drainChunk(adaptBudget: !force);
    if (pipeline.pending.isNotEmpty) {
      scheduleFlush();
    }
  }

  /// Writes at most one frame's worth of pending output, consuming the chunk
  /// that straddles the budget in place so the rest is never copied.
  void _drainChunk({bool adaptBudget = false}) {
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
    _host.writeToTerminal(frame.toString());
    if (parseClock != null) {
      parseClock.stop();
      _adaptiveChunkBudget = _terminalOutputNextAdaptiveChunkBudget(
        currentChars: chunkBudget,
        parseTime: parseClock.elapsed,
      );
    }
    _host.advanceRestore(restoreWritten);
    _host.advancePointerInputCatchUp(written);
  }

  void flushNow() {
    pipeline.cancelDeferredFlush();
    if (_host.isDisposed || pipeline.pending.isEmpty) {
      return;
    }
    pipeline.restartFlushClock();
    final pendingChars = pipeline.length;
    final buffer = StringBuffer();
    for (final segment in pipeline.pending) {
      buffer.write(segment.remainingText);
    }
    clearPending();
    _host.writeToTerminal(buffer.toString());
    // Everything queued is on screen now, including a restore this bypassed.
    _host.finishRestore();
    _host.advancePointerInputCatchUp(pendingChars);
  }

  void clearPending() {
    _hiddenCatchUpScheduled = false;
    pipeline.cancelDeferredFlush();
    pipeline.clear();
  }
}
