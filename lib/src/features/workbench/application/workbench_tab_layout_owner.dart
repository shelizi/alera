import 'dart:async';

import 'package:alera/src/features/agent_profiles/domain/agent_profile_adapters.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/application/workbench_cleared_layout_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_closed_tabs_completion_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_closed_tabs_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_explicit_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_file_tab_mutation_queue.dart';
import 'package:alera/src/features/workbench/application/workbench_file_tab_path_move_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_layout_load_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_layout_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_layout_resolver.dart';
import 'package:alera/src/features/workbench/application/workbench_layout_state.dart';
import 'package:alera/src/features/workbench/application/workbench_persisted_tab_open_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_pull_request_diff_tab_open_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_pull_request_diff_tab_store_adapter.dart';
import 'package:alera/src/features/workbench/application/workbench_replaceable_tab_editor_sessions.dart';
import 'package:alera/src/features/workbench/application/workbench_replaceable_tab_open_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_retired_tabs_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_sequenced_layout_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_close_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_placement_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_placement_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_removal_state.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_set_sync.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tab_closing_scope.dart';
import 'package:alera/src/features/workbench/application/workspace_activity_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_file_preview_kind.dart';
import 'package:alera/src/features/workbench/application/workspace_tab_service.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:uuid/uuid.dart';

part 'workbench_tab_layout_owner_opening.dart';
part 'workbench_tab_layout_owner_file_tabs.dart';
part 'workbench_tab_layout_owner_tabs.dart';

/// The narrow capabilities [WorkbenchTabLayoutOwner] needs from
/// `WorkbenchController`. Composed in `workbench_controller_internals.dart`
/// so the owner never touches Riverpod providers directly.
abstract interface class WorkbenchTabLayoutOwnerHost {
  /// The current workbench state.
  WorkbenchState readState();

  /// Replaces the current workbench state.
  void emitState(WorkbenchState next);

  /// Whether the owning controller has been disposed.
  bool get isDisposed;

  /// Tab store used for create, open, rename and close operations.
  WorkspaceTabService get workspaceTabService;

  /// Layout persistence backing the sequenced layout repository.
  WorkbenchLayoutRepository get layoutRepository;

  /// Finds a persisted tab by id for the restore flow.
  Future<WorkspaceTabRecord?> findPersistedWorkspaceTab(String tabId);

  /// Hosted-review retention used when opening or closing review tabs.
  WorkbenchHostedReviewRetentionService get hostedReviewRetention;

  /// Cleans client-local resources for explicit tab closes.
  WorkbenchExplicitResourceCleaner get explicitResourceCleaner;

  /// Releases resources for tabs retired by watcher-driven removals.
  WorkbenchRetiredTabsCleanupCoordinator get retiredTabsCleanup;

  /// Editor sessions consulted by the shared preview-tab policy.
  WorkbenchReplaceableTabEditorSessions get replaceableTabEditorSessions;

  /// Records workspace activity after a terminal tab is placed.
  WorkbenchWorkspaceActivityRecorder get workspaceActivityRecorder;

  /// Looks up a tracked workspace by id.
  Workspace? workspaceById(String workspaceId);

  /// Whether a tab watcher subscription is active for [workspaceId].
  bool isWorkspaceTabSubscriptionActive(String workspaceId);

  /// Selects [tabId] in [workspaceId] through the selection owner.
  Future<void> selectPersistedWorkspaceTab({
    required String workspaceId,
    required String tabId,
  });
}

/// Owns the tab and layout slice of the workbench: opening and closing tabs,
/// pane splits, the per-workspace layout pipeline, and watcher-driven tab set
/// reconciliation.
final class WorkbenchTabLayoutOwner {
  WorkbenchTabLayoutOwner(this._host);

  final WorkbenchTabLayoutOwnerHost _host;
  final Uuid _uuid = const Uuid();

  WorkbenchSequencedLayoutRepository? _layoutRepository;

  WorkbenchSequencedLayoutRepository get _sequencedLayoutRepository =>
      _layoutRepository ??= WorkbenchSequencedLayoutRepository(
        _host.layoutRepository,
      );

  /// Layout resolver shared with the selection owner's workspace hydration.
  WorkbenchLayoutResolver get layoutResolver =>
      WorkbenchLayoutResolver(_sequencedLayoutRepository);

  final WorkbenchLayoutLoadCoordinator _layoutLoading =
      WorkbenchLayoutLoadCoordinator();
  final WorkbenchWorkspaceTabClosingScope _tabClosingScope =
      WorkbenchWorkspaceTabClosingScope();
  final WorkbenchClearedLayoutRegistry _clearedLayouts =
      WorkbenchClearedLayoutRegistry();
  final WorkbenchFileTabMutationQueue _fileTabMutations =
      WorkbenchFileTabMutationQueue();
  final WorkspaceTabFocusHistory _tabFocusHistory = WorkspaceTabFocusHistory();

  /// Serializes explicit tab closes and workspace sleeps per workspace.
  WorkbenchWorkspaceTabClosingScope get tabClosingScope => _tabClosingScope;

  /// Workspaces whose persisted layout was cleared by a workspace sleep.
  WorkbenchClearedLayoutRegistry get clearedLayouts => _clearedLayouts;

  /// Per-workspace MRU tab history used to pick a fallback active tab.
  WorkspaceTabFocusHistory get tabFocusHistory => _tabFocusHistory;

  void ensureSelectionHasTab() {
    final state = _host.readState();
    final workspace = state.activeWorkspace;
    if (workspace == null) {
      return;
    }
    if (_tabClosingScope.isClosing(workspace.id)) {
      return;
    }
    if (state.tabsFor(workspace.id).isNotEmpty &&
        state.layoutFor(workspace.id) == null) {
      unawaited(loadLayoutForWorkspace(workspace.id));
    }
  }

  Future<void> loadLayoutForWorkspace(String workspaceId) {
    final tabService = _host.workspaceTabService;
    final tabLayoutResolver = layoutResolver;
    return _layoutLoading.load(
      workspaceId: workspaceId,
      listTabs: tabService.listTabs,
      resolveLayout: tabLayoutResolver.resolve,
      applyLayout: (layout) => applyWorkspaceLayout(layout, persist: false),
      isLoadCurrent: () =>
          !_host.isDisposed && _host.workspaceById(workspaceId) != null,
      onError: (error) {
        if (!_host.isDisposed) {
          _host.emitState(_host.readState().copyWith(error: error.toString()));
        }
      },
    );
  }

  WorkbenchLayout _layoutForMutation(
    String workspaceId,
    List<WorkspaceTabRecord> tabs,
  ) {
    return (_host.readState().layoutFor(workspaceId) ??
            WorkbenchLayout.single(
              workspaceId: workspaceId,
              tabIds: <String>[for (final tab in tabs) tab.id],
            ))
        .sanitize(tabs);
  }

  /// Applies [layout] to the state and optionally persists it.
  Future<void> applyWorkspaceLayout(
    WorkbenchLayout layout, {
    required bool persist,
  }) async {
    _host.emitState(
      applyWorkbenchLayoutState(state: _host.readState(), layout: layout),
    );
    final activeTabId = layout.activeTabId;
    if (activeTabId != null) {
      _tabFocusHistory.record(layout.workspaceId, activeTabId);
    }
    if (persist) {
      await _sequencedLayoutRepository.upsertWorkbenchLayout(layout);
    }
  }

  /// Applies [layout] in the background, reporting failures to the state.
  void applyWorkspaceLayoutInBackground(
    WorkbenchLayout layout, {
    required bool persist,
  }) {
    unawaited(
      applyWorkspaceLayout(
        layout,
        persist: persist,
      ).catchError(_recordLayoutError),
    );
  }

  void _persistLayoutInBackground(WorkbenchLayout layout) {
    unawaited(
      _sequencedLayoutRepository
          .upsertWorkbenchLayout(layout)
          .then<void>((_) {})
          .catchError(_recordLayoutError),
    );
  }

  void _recordLayoutError(Object error) {
    if (!_host.isDisposed) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
    }
  }

  /// Replaces the tab list of [workspaceId].
  void applyWorkspaceTabs(String workspaceId, List<WorkspaceTabRecord> tabs) {
    _host.emitState(
      applyWorkbenchTabsState(
        state: _host.readState(),
        workspaceId: workspaceId,
        tabs: tabs,
      ),
    );
  }

  String _newPaneGroupId() => 'pane-${_uuid.v4()}';

  /// Makes [tabId] the active tab of [workspaceId], writing into the layout
  /// when one exists and into `activeTabIdByWorkspace` otherwise.
  void activateWorkspaceTab({
    required String workspaceId,
    required String tabId,
    String? groupId,
  }) {
    final layout = _host.readState().layoutFor(workspaceId);
    final resolvedGroupId = groupId ?? layout?.groupIdForTab(tabId);
    if (layout != null && resolvedGroupId != null) {
      final nextLayout = layout.setActiveTab(
        groupId: resolvedGroupId,
        tabId: tabId,
      );
      applyWorkspaceLayoutInBackground(nextLayout, persist: true);
      return;
    }
    _host.emitState(
      applyWorkbenchActiveTabState(
        state: _host.readState(),
        workspaceId: workspaceId,
        tabId: tabId,
      ),
    );
    _tabFocusHistory.record(workspaceId, tabId);
  }

  /// Reconciles the tracked tab list and layout of [workspaceId] with the
  /// persisted set pushed by the tab watcher.
  void handleTabsChanged(String workspaceId, List<WorkspaceTabRecord> tabs) {
    if (!_host.isWorkspaceTabSubscriptionActive(workspaceId)) {
      return;
    }
    final layoutWasCleared = _clearedLayouts.contains(workspaceId);
    final plan = planWorkbenchTabSetSync(
      state: _host.readState(),
      workspaceId: workspaceId,
      tabs: tabs,
      layoutWasCleared: layoutWasCleared,
    );
    final removedTabs = plan.removedTabs;
    // A tab record that disappeared from persisted state can never reach its
    // live terminal handle again, so release the hosted-review retention and
    // client-local terminal/editor resources without terminating a PTY that
    // another client may still own.
    _host.retiredTabsCleanup.cleanup(
      workspace: _host.workspaceById(workspaceId),
      tabs: removedTabs,
    );
    if (tabs.isNotEmpty) {
      _clearedLayouts.forget(workspaceId);
    }
    _host.emitState(
      applyWorkbenchTabSetSyncPlan(state: _host.readState(), plan: plan),
    );
    if (plan.shouldLoadLayout) {
      unawaited(loadLayoutForWorkspace(workspaceId));
    }
    final layoutToPersist = plan.layoutToPersist;
    if (layoutToPersist != null) {
      _persistLayoutInBackground(layoutToPersist);
    }
    ensureSelectionHasTab();
  }
}
