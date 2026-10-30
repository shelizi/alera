import 'dart:async';

final class const TerminalRuntimeExitEvent({
  required final String workspaceId,
  required final String tabId,
  required final int exitCode,
  final bool autoCloseOnSuccess = false,
});

abstract interface class TerminalRuntimeCoordination {
  Stream<TerminalRuntimeExitEvent> get exits;

  void setActiveWorkspace(String? workspaceId);
}

abstract interface class TerminalRuntimeLifecycle {
  void closeTab(String tabId);

  void closeWorkspace(String workspaceId);

  /// Frees local terminal resources for [tabId] without terminating the PTY.
  ///
  /// Use this when persisted state no longer exposes the tab to this client
  /// but the underlying process may still be owned by another client.
  void releaseTab(String tabId);

  /// Frees local terminal resources for [workspaceId] without terminating PTYs.
  void releaseWorkspace(String workspaceId);
}

/// Read-only lease evidence from an already attached Terminal view. The
/// Workbench must never create a new PTY view or fetch a different Host
/// connection's newest token while authorizing a previously observed close.
abstract interface class TerminalRuntimeAttachedViewLeaseSource {
  /// The same view that rendered [tabId], or null if not attached locally.
  String? attachedViewLeaseIdForTab(String tabId);
}
