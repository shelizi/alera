part of 'terminal_runtime.dart';

/// PTY lifecycle, creation, generation filtering, and teardown for a
/// session handle.
extension _XtermTerminalSessionPty on _XtermTerminalSessionHandle {
  Future<bool> _startPtySession() async {
    final launches = _launchInputOwner.candidateLaunches();
    if (launches.isEmpty) {
      throw StateError(_noTerminalShellCandidatesMessage());
    }
    final agentHookEnvironment = await _launchInputOwner
        .buildAgentHookEnvironment(
          terminalSessionId: _tab.terminalSessionId,
          workspaceId: _workspace.id,
          tabId: _tab.id,
        );
    Object? lastError;
    for (final launch in launches) {
      final session = _ptySessionFactory.create(
        sessionId: _tab.terminalSessionId,
        workspaceId: _workspace.id,
        tabId: _tab.id,
      );
      final generation = ++_ptyGeneration;
      final sub = session.events.listen(
        (event) => _handlePtySessionEvent(event, generation),
      );
      _activePtyGeneration = generation;
      try {
        final prepared = await _launchInputOwner.prepareLaunch(
          launch: launch,
          workspacePath: _workspace.path,
          resolvedLoginShell: _settings.resolvedLoginShell,
          agentHookEnvironment: agentHookEnvironment,
        );
        await session.start(
          launch: prepared.workspaceLaunch,
          workingDirectory: _workspace.path,
          cols: _terminal.viewWidth,
          rows: _terminal.viewHeight,
          onProcessCreated: () => _launchInputOwner.onProcessCreated(
            terminalSessionId: _tab.terminalSessionId,
            session: session,
            workspaceLaunch: prepared.workspaceLaunch,
            interactiveShell: prepared.interactiveShell,
            initialCommand: _tab.initialCommand,
            isCurrent: () => !_disposed && _activePtyGeneration == generation,
          ),
        );
        if (_disposed || _activePtyGeneration != generation) {
          unawaited(sub.cancel());
          session.dispose();
          _prunePtyGenerationState();
          return false;
        }
        _ptySession = session;
        _ptyOutputPaused = false;
        _ptySessionSub = sub;
        bool isCurrent() =>
            !_disposed &&
            _activePtyGeneration == generation &&
            identical(_ptySession, session);
        await _refreshTerminalPulseState(session, isCurrent: isCurrent);
        if (!isCurrent()) {
          unawaited(sub.cancel());
          session.dispose();
          _prunePtyGenerationState();
          return false;
        }
        if (!_outputVisible) {
          _syncPtyOutputVisibility();
        }
        _flushPendingPtyResize();
        _running = !_exitedPtyGenerations.contains(generation);
        _prunePtyGenerationState();
        _notifySessionListeners();
        return !_disposed &&
            _activePtyGeneration == generation &&
            identical(_ptySession, session);
      } catch (error) {
        if (_disposed || _activePtyGeneration != generation) {
          unawaited(sub.cancel());
          session.dispose();
          _prunePtyGenerationState();
          return false;
        }
        lastError = error;
        _suppressedExitPtyGenerations.add(generation);
        if (_activePtyGeneration == generation) {
          _activePtyGeneration = null;
        }
        if (identical(_ptySession, session)) {
          _ptySession = null;
          _ptyOutputPaused = false;
        }
        if (identical(_ptySessionSub, sub)) {
          _ptySessionSub = null;
        }
        unawaited(sub.cancel());
        session.dispose();
        _prunePtyGenerationState();
      }
    }
    throw StateError('No desktop PTY shell could be started: $lastError');
  }

  void _handlePtySessionEvent(TerminalPtySessionEvent event, int generation) {
    if (_disposed || generation != _activePtyGeneration) {
      return;
    }
    switch (event) {
      // Output that arrives while hidden is queued, not dropped. The host
      // counts a frame as delivered the moment the client's queue takes it, so
      // discarding between losing visibility and the pause taking effect would
      // leave a gap that no later resume knows to resend.
      case TerminalPtyOutputEvent(:final data):
        _ptyOutputController.add(data);
      case TerminalPtyOutputTextEvent(:final text):
        // Already decoded by the reader isolate, so it skips the local
        // decoder entirely.
        _handleTerminalOutput(text);
      case TerminalPtySnapshotEvent(:final data, :final resetInteractionModes):
        _pendingInteractionModeReset |= resetInteractionModes;
        if (_outputVisible) {
          final shouldResetInteractionModes = _pendingInteractionModeReset;
          _preparePointerInputForSnapshot();
          _replaceTerminalWithSnapshot(
            data,
            resetInteractionModes: shouldResetInteractionModes,
          );
          _completePointerInputSnapshotCatchUp();
          _pendingInteractionModeReset = false;
        }
      case TerminalPtySnapshotTextEvent(
        :final text,
        :final resetInteractionModes,
      ):
        _pendingInteractionModeReset |= resetInteractionModes;
        if (_outputVisible) {
          final shouldResetInteractionModes = _pendingInteractionModeReset;
          _preparePointerInputForSnapshot();
          _replaceTerminalWithSnapshotText(
            text,
            resetInteractionModes: shouldResetInteractionModes,
          );
          _completePointerInputSnapshotCatchUp();
          _pendingInteractionModeReset = false;
        }
      case TerminalPtyExitEvent(:final exitCode):
        _handlePtyExit(
          exitCode: exitCode,
          generation: generation,
          notifyRuntime: event.notifyRuntime,
        );
      case TerminalPtyErrorEvent(:final error):
        _setTerminalHostError(error);
      case TerminalPtyPulseChangedEvent(:final state):
        _handleTerminalPulseChanged(state);
    }
  }

  void _handlePtyExit({
    required int exitCode,
    required int generation,
    required bool notifyRuntime,
  }) {
    if (!_exitedPtyGenerations.add(generation)) {
      return;
    }
    _running = false;
    _markTerminalPulseDisarmed();
    // An exit ends the usefulness of the restore overlay immediately. The
    // remaining restore bytes may still drain through the frame-budgeted
    // pipeline below, but they must not leave a stale "restoring" surface over
    // an already-exited terminal.
    _finishRestore();
    // Keep process-exit delivery on the same frame-budgeted pipeline as live
    // output. A process can exit immediately after a burst, and synchronously
    // parsing the entire pending backlog here would block the Flutter UI isolate.
    _queueTerminalOutput(terminalInteractionModeReset);
    _queueTerminalOutput('\n[process exited: $exitCode]\n');
    _flushPendingTerminalOutputFrame(force: true);
    _notifySessionListeners();
    if (notifyRuntime && !_suppressedExitPtyGenerations.contains(generation)) {
      _onExit(
        TerminalRuntimeExitEvent(
          workspaceId: workspaceId,
          tabId: tabId,
          exitCode: exitCode,
          autoCloseOnSuccess: _tab.autoCloseOnSuccess,
        ),
      );
    }
    unawaited(_stopPtySessionWithMode(suppressExit: true, terminate: false));
  }

  Future<void> _stopPtySession({required bool suppressExit}) async {
    await _stopPtySessionWithMode(suppressExit: suppressExit, terminate: true);
  }

  Future<void> _performStopPtySessionWithMode({
    required bool suppressExit,
    required bool terminate,
  }) async {
    _pendingPtyResizeTimer?.cancel();
    _pendingPtyResizeTimer = null;
    _pendingPtySize = null;
    _selectionCopyTimer?.cancel();
    _selectionCopyTimer = null;
    _launchInputOwner.cancelDeferredSubmitEnter(this);
    final generation = _activePtyGeneration;
    if (suppressExit && generation != null) {
      _suppressedExitPtyGenerations.add(generation);
    }
    if (_activePtyGeneration == generation) {
      _activePtyGeneration = null;
    }
    final sub = _ptySessionSub;
    _ptySessionSub = null;
    await sub?.cancel();
    final session = _ptySession;
    _ptySession = null;
    _ptyOutputPaused = false;
    if (terminate) {
      session?.terminate();
    } else {
      session?.dispose();
    }
    _prunePtyGenerationState();
  }

  void _prunePtyGenerationState() {
    final active = _activePtyGeneration;
    _exitedPtyGenerations.removeWhere((generation) => generation != active);
    _suppressedExitPtyGenerations.removeWhere(
      (generation) => generation != active,
    );
  }
}
