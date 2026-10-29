abstract interface class WorkbenchTabCloseStore {
  Future<void> closeTab(String tabId);
}

/// The caller must supply the workspace from its own tab snapshot; a stale
/// window must not close a terminal already moved into another project.
abstract interface class WorkbenchScopedTabCloseStore {
  Future<void> closeTabInWorkspace(String tabId, String workspaceId);
}
