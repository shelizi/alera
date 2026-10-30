import 'package:alera/src/features/workbench/application/workbench_layout_repository.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';

abstract interface class WorkbenchWorkspaceTabRemovalRepository {
  Future<void> removeWorkspaceTabsForWorkspace(String workspaceId);
}

/// Optional runtime-owned, workspace-scoped close. In-memory/fake repositories
/// retain their existing unscoped interface for legacy behavior.
abstract interface class WorkbenchScopedTabRemovalRepository {
  Future<void> removeWorkspaceTabForWorkspace(
    String tabId,
    String expectedWorkspaceId,
  );
}

/// An optional repository capability for scope- and receipt-bound closes.
/// Legacy repositories may close unchanged tabs but must fail closed when
/// they do not know how to authorize a previously transferred Terminal.
abstract interface class WorkbenchObservedTabRemovalRepository {
  Future<void> removeObservedWorkspaceTabForWorkspace(
    WorkspaceTabRecord observedTab,
    String expectedWorkspaceId,
  );
}

/// Optional Runtime Host capability: only a currently attached UI view may
/// request a destructive close of a previously transferred PTY.
abstract interface class WorkbenchViewBoundTabRemovalRepository {
  Future<void> removeObservedWorkspaceTabWithViewLease(
    WorkspaceTabRecord observedTab,
    String expectedWorkspaceId,
    String expectedViewLeaseId,
  );
}

abstract interface class WorkbenchRepository
    implements
        WorkbenchLayoutRepository,
        WorkbenchWorkspaceTabRemovalRepository {
  Future<List<Workspace>> listWorkspaces(String projectId);

  Stream<List<Workspace>> watchWorkspaces(String projectId);

  Future<Workspace?> findWorkspaceById(String workspaceId);

  Future<Workspace> upsertWorkspace(Workspace workspace);

  Future<Workspace> setWorkspacePinned(String workspaceId, bool isPinned);

  Future<void> removeWorkspace(String workspaceId, {bool cascadeTabs = true});

  Future<void> removeWorkspacesForProject(String projectId);

  Future<List<WorkspaceTabRecord>> listWorkspaceTabs(String workspaceId);

  Stream<List<WorkspaceTabRecord>> watchWorkspaceTabs(String workspaceId);

  Future<WorkspaceTabRecord?> findWorkspaceTabById(String tabId);

  /// Explicit renames must invalidate pending title jobs even if the text is unchanged.
  Future<WorkspaceTabRecord> upsertWorkspaceTab(
    WorkspaceTabRecord tab, {
    bool manualRename = false,
  });

  Future<void> removeWorkspaceTab(String tabId);

  Future<WorkbenchLayout?> findWorkbenchLayout(String workspaceId);

  Future<WorkbenchLayout> upsertWorkbenchLayout(WorkbenchLayout layout);

  Future<void> removeWorkbenchLayout(String workspaceId);
}
