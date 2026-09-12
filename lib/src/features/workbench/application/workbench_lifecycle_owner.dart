import 'package:alera/src/features/workbench/application/terminal_runtime_lifecycle.dart';
import 'package:alera/src/features/workbench/application/workbench_explicit_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_remove_project_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_replaceable_tab_editor_sessions.dart';
import 'package:alera/src/features/workbench/application/workbench_retired_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_retired_tabs_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_retired_workspace_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_root_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_worktree_metadata_watcher_registry.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/shared/infra/git/git_backend.dart';

/// The narrow capabilities [WorkbenchLifecycleOwner] needs from
/// `WorkbenchController`. Composed in `workbench_controller_internals.dart`
/// so the owner never touches Riverpod providers directly.
abstract interface class WorkbenchLifecycleOwnerHost {
  /// The current workbench state.
  WorkbenchState readState();

  /// Replaces the current workbench state.
  void emitState(WorkbenchState next);

  /// Whether the owning controller has been disposed.
  bool get isDisposed;

  /// Managed runtime side of terminal, tab and workspace teardown.
  TerminalRuntimeLifecycle get terminalRuntimeLifecycle;

  /// Editor sessions forgotten when a tab leaves the workbench.
  WorkbenchReplaceableTabEditorSessions get replaceableTabEditorSessions;

  /// Removes the persisted activity record of a deleted workspace.
  WorkbenchExplicitResourceIdConsumer get removeWorkspaceActivity;

  /// Clears the agent status entries of a deleted workspace.
  WorkbenchExplicitResourceIdConsumer get clearAgentWorkspace;

  /// Clears the hook-receiver session of a terminal tab when a receiver is
  /// bound.
  WorkbenchExplicitResourceIdConsumer? get clearTerminalSession;

  /// Clears the runtime overlays of a terminal session.
  WorkbenchExplicitAsyncResourceIdConsumer get clearTerminalOverlays;

  /// Git backend behind the hosted-review retention service.
  GitBackend get gitBackend;

  /// Per-workspace MRU tab history forgotten during retired cleanup.
  WorkspaceTabFocusHistory get tabFocusHistory;
}

/// Owns the resource lifecycle slice of the workbench: the watcher
/// subscription registries shared with the catalog and tab/layout owners, the
/// client-local resource cleaners and cleanup coordinators, and the teardown
/// ordering that runs when the controller is disposed.
final class WorkbenchLifecycleOwner {
  WorkbenchLifecycleOwner(this._host);

  final WorkbenchLifecycleOwnerHost _host;

  final WorkbenchRootSubscriptionRegistry _rootSubscriptions =
      WorkbenchRootSubscriptionRegistry();
  final WorkbenchWorkspaceSubscriptionRegistry _workspaceSubscriptions =
      WorkbenchWorkspaceSubscriptionRegistry();
  final WorkbenchTabSubscriptionRegistry _tabSubscriptions =
      WorkbenchTabSubscriptionRegistry();
  final WorkbenchWorktreeMetadataWatcherRegistry
  _worktreeMetadataWatcherRegistry = WorkbenchWorktreeMetadataWatcherRegistry();

  /// Root watchers: projects, sections and view prefs.
  WorkbenchRootSubscriptionRegistry get rootSubscriptions => _rootSubscriptions;

  /// Per-project workspace watchers shared with the catalog owner.
  WorkbenchWorkspaceSubscriptionRegistry get workspaceSubscriptions =>
      _workspaceSubscriptions;

  /// Per-workspace tab watchers shared with the catalog and tab/layout
  /// owners.
  WorkbenchTabSubscriptionRegistry get tabSubscriptions => _tabSubscriptions;

  /// Worktree metadata watchers shared with the catalog owner.
  WorkbenchWorktreeMetadataWatcherRegistry
  get worktreeMetadataWatcherRegistry => _worktreeMetadataWatcherRegistry;

  /// Whether a tab watcher subscription is active for [workspaceId].
  bool isWorkspaceTabSubscriptionActive(String workspaceId) =>
      _tabSubscriptions.contains(workspaceId);

  /// Hosted-review retention released when tabs or workspaces leave the
  /// catalog.
  late final WorkbenchHostedReviewRetentionService hostedReviewRetention =
      WorkbenchHostedReviewRetentionService(gitBackend: _host.gitBackend);

  /// Cleans client-local resources for explicit tab and workspace closes.
  late final WorkbenchExplicitResourceCleaner explicitResourceCleaner =
      WorkbenchExplicitResourceCleaner(
        runtimeLifecycle: _host.terminalRuntimeLifecycle,
        forgetEditorSession: _host.replaceableTabEditorSessions.forget,
        removeWorkspaceActivity: _host.removeWorkspaceActivity,
        clearAgentWorkspace: _host.clearAgentWorkspace,
        // The hook receiver can bind after this owner is constructed, so the
        // lookup stays per call rather than captured here.
        clearTerminalSession: (sessionId) =>
            _host.clearTerminalSession?.call(sessionId),
        clearTerminalOverlays: _host.clearTerminalOverlays,
      );

  late final WorkbenchRetiredResourceCleaner _retiredResourceCleaner =
      WorkbenchRetiredResourceCleaner(
        runtimeLifecycle: _host.terminalRuntimeLifecycle,
        forgetEditorSession: _host.replaceableTabEditorSessions.forget,
        clearTerminalSession: (sessionId) =>
            _host.clearTerminalSession?.call(sessionId),
      );

  /// Releases resources for workspaces retired by watcher-driven removals.
  late final WorkbenchRetiredWorkspaceCleanupCoordinator
  retiredWorkspaceCleanup = WorkbenchRetiredWorkspaceCleanupCoordinator(
    releaseHostedReviewTabsInBackground:
        hostedReviewRetention.releaseTabsInBackground,
    forgetFocusHistory: _host.tabFocusHistory.forget,
    releaseLocalWorkspace: _retiredResourceCleaner.releaseWorkspace,
  );

  /// Releases resources for tabs retired by watcher-driven removals.
  late final WorkbenchRetiredTabsCleanupCoordinator retiredTabsCleanup =
      WorkbenchRetiredTabsCleanupCoordinator(
        releaseHostedReviewTabsInBackground:
            hostedReviewRetention.releaseTabsInBackground,
        releaseLocalTabs: _retiredResourceCleaner.releaseTabs,
      );

  /// Closes the managed runtime side of a removed project workspace.
  WorkbenchRemovedProjectWorkspaceCloser get removedProjectWorkspaceCloser =>
      _host.terminalRuntimeLifecycle.closeWorkspace;

  /// Cancels every watcher the workbench started. Catalog watchers stop first
  /// so tab callbacks stop after workspace callbacks; the shared tab
  /// subscription registry is cancelled last.
  void dispose() {
    _rootSubscriptions.cancelAll();
    _worktreeMetadataWatcherRegistry.disposeAll();
    _workspaceSubscriptions.cancelAll();
    _tabSubscriptions.cancelAll();
  }
}
