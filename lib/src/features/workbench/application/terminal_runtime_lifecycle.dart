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
