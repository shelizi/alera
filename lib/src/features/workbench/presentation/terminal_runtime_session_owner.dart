part of 'terminal_runtime.dart';

/// The narrow capabilities needed by [TerminalRuntimeSessionOwner].
///
/// The owner keeps session state and lifecycle policy here while the runtime
/// supplies the concrete handle and forwards effects that belong to the host.
abstract interface class TerminalRuntimeSessionOwnerHost {
  TerminalSettings get terminalSettings;

  TerminalSessionHandle createSession({
    required TerminalRuntimeSessionOwner owner,
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  });

  void disposeSession(
    TerminalSessionHandle session, {
    required bool terminatePty,
  });

  void forwardSessionExit(TerminalRuntimeExitEvent event);
}

/// Owns the durable terminal-session handles and their view attachment policy.
///
/// A handle leaves this registry when its view is released or evicted, but the
/// host-side durable session is terminated only for an explicit close.
final class TerminalRuntimeSessionOwner
    implements TerminalRuntimeViewBufferOwnerHost {
  TerminalRuntimeSessionOwner(
    this._host, {
    TerminalRuntimeViewBufferOwner Function(
      TerminalRuntimeViewBufferOwnerHost host,
    )?
    viewBufferOwnerBuilder,
  }) {
    _viewBufferOwner =
        viewBufferOwnerBuilder?.call(this) ??
        TerminalRuntimeViewBufferOwner(this);
  }

  final TerminalRuntimeSessionOwnerHost _host;
  late final TerminalRuntimeViewBufferOwner _viewBufferOwner;
  final Map<String, _XtermTerminalSessionHandle> _sessions =
      <String, _XtermTerminalSessionHandle>{};

  @override
  TerminalSettings get terminalSettings => _host.terminalSettings;

  @override
  Iterable<TerminalSessionHandle> get liveSessions => _sessions.values;

  @override
  void evictSession(String tabId) {
    final session = _sessions[tabId];
    if (session == null) {
      return;
    }
    // The parser worker already owns the authoritative terminal model while a
    // tab is hidden. Drop only the duplicate UI cell buffer and keep the PTY +
    // worker alive; reveal can rebuild the replica from one packed worker
    // snapshot without reparsing the host's full ANSI scrollback.
    if (session._evictParserWorkerUiBuffer()) {
      return;
    }
    _sessions.remove(tabId);
    _disposeSession(session, terminatePty: false);
  }

  TerminalSessionHandle sessionFor({
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  }) {
    final session = _sessions.putIfAbsent(tab.id, () {
      final handle = _host.createSession(
        owner: this,
        workspace: workspace,
        tab: tab,
      );
      final xtermHandle = handle as _XtermTerminalSessionHandle;
      xtermHandle.setAppForeground(_viewBufferOwner.isAppForeground);
      // Apply only at session creation so toggling the setting later does not
      // reopen composers the user already closed.
      if (_host.terminalSettings.showComposerByDefault) {
        xtermHandle.composerController.show();
      }
      return xtermHandle;
    });
    return session.sync(workspace: workspace, tab: tab);
  }

  TerminalSessionHandle? peekSession(String tabId) => _sessions[tabId];

  void requestFocus({
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  }) {
    sessionFor(workspace: workspace, tab: tab).requestFocus();
  }

  void applySettings() {
    final settings = _host.terminalSettings;
    for (final session in _sessions.values) {
      session.applySettings(settings);
    }
    // Lowering the budget in settings has to take effect now, not at the next
    // workspace switch.
    _viewBufferOwner.enforceBufferBudget();
  }

  void setActiveWorkspace(String? workspaceId) {
    _viewBufferOwner.setActiveWorkspace(workspaceId);
  }

  void setAppForeground(bool foreground) {
    _viewBufferOwner.setAppForeground(foreground);
  }

  void _handleVisibilityChanged(_XtermTerminalSessionHandle handle) {
    _viewBufferOwner.handleVisibilityChanged(handle);
  }

  void _handleSessionExit(TerminalRuntimeExitEvent event) {
    _host.forwardSessionExit(event);
  }

  void closeTab(String tabId) {
    final session = _sessions.remove(tabId);
    if (session != null) {
      _disposeSession(session, terminatePty: true);
    }
  }

  void closeWorkspace(String workspaceId) {
    final removed = _sessions.entries
        .where((entry) => entry.value.workspaceId == workspaceId)
        .map((entry) => entry.key)
        .toList(growable: false);
    for (final tabId in removed) {
      final session = _sessions.remove(tabId);
      if (session != null) {
        _disposeSession(session, terminatePty: true);
      }
    }
  }

  void releaseTab(String tabId) {
    final session = _sessions.remove(tabId);
    if (session != null) {
      _disposeSession(session, terminatePty: false);
    }
  }

  void releaseWorkspace(String workspaceId) {
    final removed = _sessions.entries
        .where((entry) => entry.value.workspaceId == workspaceId)
        .map((entry) => entry.key)
        .toList(growable: false);
    for (final tabId in removed) {
      releaseTab(tabId);
    }
  }

  void dispose() {
    for (final session in _sessions.values) {
      _disposeSession(session, terminatePty: false);
    }
    _sessions.clear();
  }

  void _disposeSession(
    _XtermTerminalSessionHandle session, {
    required bool terminatePty,
  }) {
    _host.disposeSession(session, terminatePty: terminatePty);
  }

  Future<void> _ensureTerminalSessionStarted(
    _XtermTerminalSessionHandle handle,
  ) async {
    if (handle._started || handle._starting) {
      return;
    }
    final attempt = ++handle._startAttempt;
    handle._starting = true;
    handle._operation = TerminalSessionOperation.starting;
    handle._errorMessage = null;
    handle._notifySessionListeners();
    try {
      if (!_isSupportedNativeDesktopTerminalPlatform) {
        throw UnsupportedError(
          'Terminal sessions require a native desktop PTY path.',
        );
      }
      final started = await handle._startPtySession();
      if (!handle._disposed && attempt == handle._startAttempt && started) {
        handle._started = true;
      }
    } catch (error) {
      if (!handle._disposed && attempt == handle._startAttempt) {
        handle._errorMessage = error.toString();
      }
    } finally {
      if (!handle._disposed && attempt == handle._startAttempt) {
        handle._starting = false;
        handle._operation = null;
        handle._notifySessionListeners();
      }
    }
  }

  Future<void> _reconnectTerminalSession(
    _XtermTerminalSessionHandle handle,
  ) async {
    final session = handle._ptySession;
    if (session is! RecoverableTerminalPtySession) {
      await _restartTerminalSession(handle);
      return;
    }
    await _recoverTerminalSession(
      handle,
      operation: .reconnecting,
      recover: session.reconnect,
    );
  }

  Future<void> _restartTerminalSession(
    _XtermTerminalSessionHandle handle,
  ) async {
    final session = handle._ptySession;
    if (session is RecoverableTerminalPtySession && session.supportsRestart) {
      await _recoverTerminalSession(
        handle,
        operation: .restarting,
        clearExitedGeneration: true,
        recover: () async {
          await session.restartProcess();
          if (!handle._disposed) {
            handle._resetPointerInputSynchronization();
          }
        },
      );
      return;
    }
    handle._startAttempt += 1;
    handle._errorMessage = null;
    handle._started = false;
    handle._starting = false;
    handle._operation = TerminalSessionOperation.restarting;
    handle._running = false;
    handle._notifySessionListeners();
    await handle._stopPtySession(suppressExit: true);
    handle._resetPointerInputSynchronization();
    if (handle._disposed) {
      return;
    }
    handle._operation = null;
    await _ensureTerminalSessionStarted(handle);
  }

  Future<void> _recoverTerminalSession(
    _XtermTerminalSessionHandle handle, {
    required TerminalSessionOperation operation,
    bool clearExitedGeneration = false,
    required Future<void> Function() recover,
  }) async {
    if (handle._disposed || handle._operation != null) {
      return;
    }
    handle._errorMessage = null;
    handle._operation = operation;
    handle._starting = true;
    handle._notifySessionListeners();
    final generationBeforeRecovery = handle._activePtyGeneration;
    final restoredExitedGeneration =
        clearExitedGeneration &&
        generationBeforeRecovery != null &&
        handle._exitedPtyGenerations.remove(generationBeforeRecovery);
    try {
      await recover();
      if (!handle._disposed) {
        final generation = handle._activePtyGeneration;
        final pulseSession = handle._ptySession;
        if (pulseSession == null) {
          handle._markTerminalPulseDisarmed();
        } else {
          await handle._refreshTerminalPulseState(
            pulseSession,
            isCurrent: () =>
                !handle._disposed &&
                generation != null &&
                handle._activePtyGeneration == generation &&
                identical(handle._ptySession, pulseSession),
          );
        }
        handle._running =
            generation != null &&
            handle._activePtyGeneration == generation &&
            identical(handle._ptySession, pulseSession) &&
            !handle._exitedPtyGenerations.contains(generation);
      }
    } catch (error) {
      if (!handle._disposed) {
        if (restoredExitedGeneration &&
            handle._activePtyGeneration == generationBeforeRecovery) {
          handle._exitedPtyGenerations.add(generationBeforeRecovery);
        }
        handle._errorMessage = 'Terminal host unavailable: $error';
      }
    } finally {
      if (!handle._disposed) {
        handle._starting = false;
        handle._operation = null;
        handle._notifySessionListeners();
      }
    }
  }
}
