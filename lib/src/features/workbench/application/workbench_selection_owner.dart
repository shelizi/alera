import 'dart:async';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_navigation_history_service.dart';
import 'package:alera/src/features/workbench/application/workbench_selection_state.dart';
import 'package:alera/src/features/workbench/application/workbench_source_control_folder_focus_service.dart';
import 'package:alera/src/features/workbench/application/workbench_source_control_root_prefs.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_selection_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_selection_hydrator.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

/// The narrow capabilities [WorkbenchSelectionOwner] needs from
/// `WorkbenchController`. Composed in `workbench_controller_internals.dart`
/// so the owner never touches Riverpod providers directly.
abstract interface class WorkbenchSelectionOwnerHost {
  /// The current workbench state.
  WorkbenchState readState();

  /// Replaces the current workbench state.
  void emitState(WorkbenchState next);

  /// Whether the owning controller has been disposed.
  bool get isDisposed;

  /// Tab store used to hydrate a workspace selection.
  WorkbenchWorkspaceSelectionTabStore get selectionTabStore;

  /// Layout resolver used to hydrate a workspace selection.
  WorkbenchWorkspaceSelectionLayoutResolver get selectionLayoutResolver;

  /// Probes whether a folder is a git repository.
  WorkbenchGitRepositoryProbe get gitRepositoryProbe;

  /// Replaces the tab list of [workspaceId].
  void applyWorkspaceTabs(String workspaceId, List<WorkspaceTabRecord> tabs);

  /// Applies [layout] to the state and optionally persists it.
  Future<void> applyWorkspaceLayout(
    WorkbenchLayout layout, {
    required bool persist,
  });

  /// Applies [layout] in the background, reporting failures to the state.
  void applyWorkspaceLayoutInBackground(
    WorkbenchLayout layout, {
    required bool persist,
  });

  /// Makes [tabId] the active tab of [workspaceId], writing into the layout
  /// when one exists and into `activeTabIdByWorkspace` otherwise.
  void activateWorkspaceTab({
    required String workspaceId,
    required String tabId,
    String? groupId,
  });

  /// Replaces the view preferences and persists them in the background.
  void updateViewPrefs(WorkbenchViewPrefs prefs);
}

/// Owns the selection and navigation slice of the workbench: which project,
/// workspace and tab are active, the worktree navigation history, and the
/// sidebar search/collapse selection flags.
final class WorkbenchSelectionOwner {
  WorkbenchSelectionOwner(this._host);

  final WorkbenchSelectionOwnerHost _host;
  final WorkbenchNavigationHistoryService _navigationHistory =
      WorkbenchNavigationHistoryService();

  bool get canGoBack => _navigationHistory.canGoBack(_host.readState());

  bool get canGoForward => _navigationHistory.canGoForward(_host.readState());

  void pruneNavigationHistory() {
    _navigationHistory.prune(_host.readState());
  }

  void _notifyNavigationHistoryChanged() {
    if (!_host.isDisposed) {
      _host.emitState(_host.readState().copyWith());
    }
  }

  Future<void> selectWorkspace({
    required Project project,
    required Workspace workspace,
    required bool ensureInitialTerminal,
    bool recordHistory = true,
  }) =>
      WorkbenchWorkspaceSelectionCoordinator(
        activateSelection: () {
          _host.emitState(
            selectWorkbenchWorkspace(
              state: _host.readState(),
              project: project,
              workspace: workspace,
            ),
          );
        },
        hydrate: ({required workspaceId, required ensureInitialTerminal}) =>
            WorkbenchWorkspaceSelectionHydrator(
              tabStore: _host.selectionTabStore,
              layoutResolver: _host.selectionLayoutResolver,
            ).hydrate(
              workspaceId: workspaceId,
              ensureInitialTerminal: ensureInitialTerminal,
            ),
        applyTabs: (tabs) => _host.applyWorkspaceTabs(workspace.id, tabs),
        applyLayout: (layout) =>
            _host.applyWorkspaceLayout(layout, persist: false),
        isSelectionCurrent: () {
          if (_host.isDisposed) {
            return false;
          }
          final state = _host.readState();
          return state.activeProjectId == project.id &&
              state.activeWorkspaceId == workspace.id;
        },
        recordHistory: () =>
            _navigationHistory.record(project: project, workspace: workspace),
        notifyHistoryChanged: _notifyNavigationHistoryChanged,
      ).select(
        workspaceId: workspace.id,
        ensureInitialTerminal: ensureInitialTerminal,
        shouldRecordHistory: recordHistory,
      );

  void activateProject(Project project) {
    _host.emitState(
      activateWorkbenchProject(state: _host.readState(), project: project),
    );
  }

  Future<void> selectWorkspaceTab({
    required String workspaceId,
    required String tabId,
  }) async {
    final current = _host.readState();
    final workspace = current.workspacesByProject.values
        .expand((workspaces) => workspaces)
        .where((workspace) => workspace.id == workspaceId)
        .firstOrNull;
    final project = workspace == null
        ? null
        : _projectById(current.projects, workspace.projectId);
    if (workspace == null || project == null) return;
    if (current.activeWorkspaceId != workspaceId) {
      await selectWorkspace(
        project: project,
        workspace: workspace,
        ensureInitialTerminal: true,
      );
      if (!_host
          .readState()
          .tabsFor(workspaceId)
          .any((tab) => tab.id == tabId)) {
        return;
      }
    }
    final groupId = _host
        .readState()
        .layoutFor(workspaceId)
        ?.groupIdForTab(tabId);
    _host.activateWorkspaceTab(
      workspaceId: workspaceId,
      tabId: tabId,
      groupId: groupId,
    );
  }

  Future<void> goBack() async {
    final changed = await _navigationHistory.navigateBack(
      _host.readState(),
      select: (selection) => selectWorkspace(
        project: selection.project,
        workspace: selection.workspace,
        ensureInitialTerminal: true,
        recordHistory: false,
      ),
    );
    if (changed) {
      _notifyNavigationHistoryChanged();
    }
  }

  Future<void> goForward() async {
    final changed = await _navigationHistory.navigateForward(
      _host.readState(),
      select: (selection) => selectWorkspace(
        project: selection.project,
        workspace: selection.workspace,
        ensureInitialTerminal: true,
        recordHistory: false,
      ),
    );
    if (changed) {
      _notifyNavigationHistoryChanged();
    }
  }

  void setActiveTab({required String workspaceId, required String tabId}) {
    final layout = _host.readState().layoutFor(workspaceId);
    final groupId = layout?.groupIdForTab(tabId);
    _host.activateWorkspaceTab(
      workspaceId: workspaceId,
      tabId: tabId,
      groupId: groupId,
    );
  }

  void setActiveWorkspaceTab({
    required String workspaceId,
    required String groupId,
    required String tabId,
  }) {
    _host.activateWorkspaceTab(
      workspaceId: workspaceId,
      groupId: groupId,
      tabId: tabId,
    );
  }

  /// Promotes [groupId] to the workspace's active group when a pane in it
  /// receives real keyboard focus. Applied in memory only; the next explicit
  /// user action persists the latest layout.
  void focusWorkbenchGroup({
    required String workspaceId,
    required String groupId,
  }) {
    final layout = _host.readState().layoutFor(workspaceId);
    if (layout == null || layout.activeGroupId == groupId) {
      return;
    }
    final tabId = layout.groups[groupId]?.activeTabId;
    if (tabId == null) {
      return;
    }
    final nextLayout = layout.setActiveTab(groupId: groupId, tabId: tabId);
    _host.applyWorkspaceLayoutInBackground(nextLayout, persist: false);
  }

  void setSearchQuery(String query) {
    final state = _host.readState();
    if (state.searchQuery == query) {
      return;
    }
    _host.emitState(state.copyWith(searchQuery: query));
  }

  void setCollapsed(bool value) {
    final state = _host.readState();
    if (state.collapsed == value) {
      return;
    }
    _host.emitState(state.copyWith(collapsed: value));
  }

  Future<bool> focusSourceControlFolder({
    required Workspace workspace,
    required String relativePath,
  }) async {
    final project = _projectById(
      _host.readState().projects,
      workspace.projectId,
    );
    if (project == null) {
      return false;
    }
    final relativeRoot =
        await WorkbenchSourceControlFolderFocusService(
          gitRepositoryProbe: _host.gitRepositoryProbe,
        ).resolve(
          project: project,
          workspace: workspace,
          relativePath: relativePath,
          isSelectionActive: () {
            final state = _host.readState();
            return state.activeProjectId == project.id &&
                state.activeWorkspaceId == workspace.id;
          },
        );
    if (relativeRoot == null) {
      return false;
    }
    _host.updateViewPrefs(
      focusWorkbenchSourceControlRootPrefs(
        prefs: _host.readState().viewPrefs,
        workspaceId: workspace.id,
        relativeRoot: relativeRoot,
      ),
    );
    _host.emitState(_host.readState().copyWith(error: null));
    return true;
  }

  void clearFocusedSourceControlFolder({required Workspace workspace}) {
    final prefs = clearWorkbenchSourceControlRootPrefs(
      prefs: _host.readState().viewPrefs,
      workspaceId: workspace.id,
    );
    if (prefs != null) {
      _host.updateViewPrefs(prefs);
    }
  }

  Project? _projectById(Iterable<Project> projects, String? projectId) {
    if (projectId == null) {
      return null;
    }
    for (final project in projects) {
      if (project.id == projectId) {
        return project;
      }
    }
    return null;
  }
}
