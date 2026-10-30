import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

abstract interface class WorkbenchTabCloseStore {
  Future<void> closeTab(String tabId);
}

/// The caller must supply the workspace from its own tab snapshot; a stale
/// window must not close a terminal already moved into another project.
abstract interface class WorkbenchScopedTabCloseStore {
  Future<void> closeTabInWorkspace(String tabId, String workspaceId);
}

/// Close using the Tab record observed *before* a user clicked close.
/// A transferred PTY needs that observation's receipt to prevent A->B->A.
abstract interface class WorkbenchObservedTabCloseStore {
  Future<void> closeObservedTabInWorkspace(
    WorkspaceTabRecord observedTab,
    String expectedWorkspaceId,
  );
}
