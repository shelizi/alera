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
  void discardPointerInputCatchUp({required int offset, required int chars});
}

/// Owns the output pipeline and drives all queuing, trimming, scheduling, and
/// draining for one terminal session.
///
/// Extracted from the free functions in terminal_runtime_output_batching.dart
/// so the handle no longer owns raw pipeline state or batching policy.
class _TerminalSessionOutputPump {
  _TerminalSessionOutputPump(this._host);

  final _TerminalSessionOutputHost _host;
  final _TerminalOutputPipeline pipeline = _TerminalOutputPipeline();

  void queue(
    String data, {
    _TerminalOutputSource source = _TerminalOutputSource.live,
  }) {
    if (data.isEmpty || _host.isDisposed) {
      return;
    }
    pipeline.add(_TerminalOutputSegment(data, source));
    if (source == _TerminalOutputSource.live) {
      _trimLiveBacklog();
    }
    scheduleFlush();
  }

  void _trimLiveBacklog() {
    while (pipeline.liveLength > _terminalOutputMaxPendingChars) {
      final segment = pipeline.pending.firstWhere(
        (candidate) => candidate.source == _TerminalOutputSource.live,
      );
      final offset = pipeline.offsetOf(segment);
      final excess = pipeline.liveLength - _terminalOutputMaxPendingChars;
      final trim = excess < segment.remaining ? excess : segment.remaining;
      final target = segment.head + trim;
      final nextHead = _terminalOutputHeadTrimStart(segment.text, target);
      final dropped = nextHead - segment.head;
      pipeline.consume(segment, dropped);
      _host.discardPointerInputCatchUp(offset: offset, chars: dropped);
    }
  }

  void scheduleFlush() {
    // A hidden terminal keeps its backlog but pays no frame time for it; the
    // backlog is drained when it becomes visible again.
    if (pipeline.flushScheduled ||
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
    _drainChunk();
    if (pipeline.pending.isNotEmpty) {
      scheduleFlush();
    }
  }

  /// Writes at most one frame's worth of pending output, consuming the chunk
  /// that straddles the budget in place so the rest is never copied.
  void _drainChunk() {
    final pending = pipeline.pending;
    if (pending.isEmpty) {
      return;
    }
    final frame = StringBuffer();
    var written = 0;
    var restoreWritten = 0;
    while (pending.isNotEmpty && written < _terminalOutputMaxCharsPerFrame) {
      final segment = pending.first;
      final head = segment.head;
      final available = segment.remaining;
      final remaining = _terminalOutputMaxCharsPerFrame - written;
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
    _host.writeToTerminal(frame.toString());
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
    pipeline.cancelDeferredFlush();
    pipeline.clear();
  }
}
