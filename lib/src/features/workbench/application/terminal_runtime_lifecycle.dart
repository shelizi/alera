abstract interface class TerminalRuntimeLifecycle {
  void closeTab(String tabId);

  void closeWorkspace(String workspaceId);

  void releaseTab(String tabId);

  void releaseWorkspace(String workspaceId);
}
