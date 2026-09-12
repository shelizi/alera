part of 'terminal_runtime.dart';

abstract interface class TerminalRuntimeViewBufferOwnerHost {
  TerminalSettings get terminalSettings;

  Iterable<TerminalSessionHandle> get liveSessions;

  void evictSession(String tabId);
}

/// Owns the visibility leases, foreground lifecycle, and buffer budget policy
/// for terminal sessions.
///
/// Handles are detached (evicted) when the estimated total buffer memory
/// exceeds the configured budget, but their durable host sessions are preserved.
final class TerminalRuntimeViewBufferOwner {
  TerminalRuntimeViewBufferOwner(this._host);

  final TerminalRuntimeViewBufferOwnerHost _host;
  String? _activeWorkspaceId;
  bool _appForeground = true;

  bool get isAppForeground => _appForeground;
  String? get activeWorkspaceId => _activeWorkspaceId;

  void setActiveWorkspace(String? workspaceId) {
    if (_activeWorkspaceId == workspaceId) {
      return;
    }
    _activeWorkspaceId = workspaceId;
    enforceBufferBudget();
  }

  void setAppForeground(bool foreground) {
    if (_appForeground == foreground) {
      return;
    }
    _appForeground = foreground;
    for (final session in _host.liveSessions) {
      if (session is _XtermTerminalSessionHandle) {
        session.setAppForeground(foreground);
      }
    }
  }

  void handleVisibilityChanged(TerminalSessionHandle session) {
    if (session.isVisible) {
      return;
    }
    enforceBufferBudget();
  }

  void enforceBufferBudget() {
    final budget = TerminalBufferBudget(
      budgetBytes: _host.terminalSettings.bufferBudgetMegabytes * 1024 * 1024,
    );
    final sessions = _host.liveSessions.toList(growable: false);
    if (budget.isUnbounded || sessions.isEmpty) {
      return;
    }
    final pinned = <String>{
      for (final session in sessions)
        if (session.isVisible) session.tabId,
    };
    final evictions = budget.selectEvictions(
      live: <TerminalBufferUsage>[
        for (final session in sessions) session.bufferUsage,
      ],
      pinnedTabIds: pinned,
    );
    for (final tabId in evictions) {
      _host.evictSession(tabId);
    }
  }
}
