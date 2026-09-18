part of 'terminal_runtime.dart';

/// Keeps pointer events behind the output that determines terminal mouse mode.
///
/// A backgrounded app can leave the emulator in a TUI mouse mode while the
/// host has already returned to the shell. Missed cleanup bytes arrive during
/// host resume, so clicks stay suspended until that prefix has been parsed.
extension _TerminalPointerSynchronization on _XtermTerminalSessionHandle {
  void _syncPtyOutputVisibility() {
    final generation = ++_outputVisibilityGeneration;
    final session = _ptySession;
    if (_disposed || session == null) {
      _pointerInputResumePending = false;
      _refreshPointerInputSuspension();
      return;
    }

    // Normal hidden tabs keep host delivery flowing so long-running shells do
    // not outgrow the host ring. Once the buffer budget has evicted this tab's
    // duplicate UI replica, however, the retained parser worker is a precise
    // structured checkpoint. Park host delivery until reveal so reconnect can
    // consume only the missed delta without paying a full ANSI reparse.
    final paused =
        !_visibility.isAppForeground || (_uiBufferEvicted && !_outputVisible);
    final pauseStateChanged = _ptyOutputPaused != paused;
    _pointerInputResumePending = _outputVisible && !paused && pauseStateChanged;
    _refreshPointerInputSuspension();
    if (!pauseStateChanged) {
      return;
    }
    // Record the requested state before awaiting so rapid foreground changes
    // enqueue both edges instead of a late pause suppressing its matching
    // resume. TerminalHostPtySession serializes those requests.
    _ptyOutputPaused = paused;
    unawaited(
      _applyPtyOutputVisibility(
        session: session,
        paused: paused,
        generation: generation,
      ),
    );
  }

  Future<void> _applyPtyOutputVisibility({
    required TerminalPtySession session,
    required bool paused,
    required int generation,
  }) async {
    try {
      await session.setOutputPaused(paused);
    } catch (error) {
      if (!_disposed &&
          identical(_ptySession, session) &&
          generation == _outputVisibilityGeneration) {
        // The optimistic marker above is what keeps a rapid background ->
        // foreground transition ordered correctly. If this request itself
        // failed and has not been superseded, roll the marker back so the next
        // visibility sync can retry instead of believing the host is already
        // in the requested state.
        if (_ptyOutputPaused == paused) {
          _ptyOutputPaused = !paused;
        }
        _setTerminalHostError(error);
      }
      return;
    }
    if (_disposed ||
        paused ||
        !_outputVisible ||
        !identical(_ptySession, session) ||
        generation != _outputVisibilityGeneration) {
      return;
    }
    _schedulePointerInputResume(generation);
  }

  void _schedulePointerInputResume(int generation) {
    SchedulerBinding.instance.scheduleFrameCallback((_) {
      if (_disposed ||
          !_outputVisible ||
          generation != _outputVisibilityGeneration) {
        return;
      }
      _pointerInputResumePending = false;
      if (_pump.pipeline.length > _pointerInputCatchUpChars) {
        _pointerInputCatchUpChars = _pump.pipeline.length;
      }
      _refreshPointerInputSuspension();
    });
    SchedulerBinding.instance.ensureVisualUpdate();
  }

  void _preparePointerInputForSnapshot() {
    _pointerInputCatchUpChars = 0;
    _setPointerInputSuspended(true);
  }

  void _completePointerInputSnapshotCatchUp() {
    _pointerInputCatchUpChars = _pump.pipeline.length;
    _refreshPointerInputSuspension();
  }

  void _completePointerInputDirectSnapshotHydration() {
    _pointerInputCatchUpChars = 0;
    _refreshPointerInputSuspension();
  }

  void _advancePointerInputCatchUp(int chars) {
    if (chars <= 0 || _pointerInputCatchUpChars <= 0) {
      return;
    }
    if (chars >= _pointerInputCatchUpChars) {
      _pointerInputCatchUpChars = 0;
    } else {
      _pointerInputCatchUpChars -= chars;
    }
    _refreshPointerInputSuspension();
  }

  void _resetPointerInputSynchronization() {
    _outputVisibilityGeneration += 1;
    _pointerInputResumePending = false;
    _pointerInputCatchUpChars = 0;
    _refreshPointerInputSuspension();
  }

  void _refreshPointerInputSuspension() {
    _setPointerInputSuspended(
      !_outputVisible ||
          _pointerInputResumePending ||
          _pointerInputCatchUpChars > 0,
    );
  }

  void _setPointerInputSuspended(bool suspended) {
    if (_terminalController.suspendedPointerInputs == suspended) {
      return;
    }
    _terminalController.setSuspendPointerInput(suspended);
  }
}
