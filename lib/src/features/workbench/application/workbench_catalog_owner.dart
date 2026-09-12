import 'dart:async';

import 'package:alera/src/features/projects/application/projects_service.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_bootstrap_gate.dart';
import 'package:alera/src/features/workbench/application/workbench_bootstrap_orchestrator.dart';
import 'package:alera/src/features/workbench/application/workbench_cleared_layout_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_delete_workspace_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_explicit_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_main_workspace_preparation_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_project_set_sync.dart';
import 'package:alera/src/features/workbench/application/workbench_project_workspace_subscription_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_prompt_workspace_completion_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_remove_project_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_retired_workspace_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_root_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_section_state.dart';
import 'package:alera/src/features/workbench/application/workbench_selection_state.dart';
import 'package:alera/src/features/workbench/application/workbench_serial_mutation_queue.dart';
import 'package:alera/src/features/workbench/application/workbench_sleep_workspace_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_sleep_workspace_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_view_prefs_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_creation_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_creation_parent_link_service.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_parent_mutation_queue.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_parent_update_service.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_set_sync.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tab_closing_scope.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tab_subscription_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tag_creation_service.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tag_mutation_queue.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tag_update_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tag_update_service.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tree_pin_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_worktree_metadata_watcher_registry.dart';
import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';
import 'package:alera/src/features/workbench/application/workspace_section_repository.dart';
import 'package:alera/src/features/workbench/application/workspace_service.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';
import 'package:alera/src/features/workbench/domain/workspace_section.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

part 'workbench_catalog_owner_projects.dart';
part 'workbench_catalog_owner_sections.dart';
part 'workbench_catalog_owner_sync.dart';
part 'workbench_catalog_owner_workspace_creation.dart';

/// The narrow capabilities [WorkbenchCatalogOwner] needs from
/// `WorkbenchController`. Composed in `workbench_controller_internals.dart`
/// so the owner never touches Riverpod providers directly.
abstract interface class WorkbenchCatalogOwnerHost {
  /// The current workbench state.
  WorkbenchState readState();

  /// Replaces the current workbench state.
  void emitState(WorkbenchState next);

  /// Whether the owning controller has been disposed.
  bool get isDisposed;

  /// Project catalog persistence: add, clone, rename, remove, watch.
  ProjectsService get projectsService;

  /// Workspace catalog persistence: watchers, pins and tab removal.
  WorkbenchRepository get workbenchRepository;

  /// Workspace lifecycle service backing creation, removal and reconcile.
  WorkspaceService get workspaceService;

  /// Workspace graph persistence backing tags and parent relations.
  WorkspaceGraphRepository get workspaceGraphRepository;

  /// View preferences repository, null when persistence is unavailable.
  WorkbenchViewPrefsRepository? get viewPrefsRepository;

  /// Hosted-review retention released when workspaces leave the catalog.
  WorkbenchHostedReviewRetentionService get hostedReviewRetention;

  /// Cleans client-local resources for explicit workspace removal.
  WorkbenchExplicitResourceCleaner get explicitResourceCleaner;

  /// Releases resources for workspaces retired by watcher-driven removals.
  WorkbenchRetiredWorkspaceCleanupCoordinator get retiredWorkspaceCleanup;

  /// Closes the managed runtime side of a removed project workspace.
  WorkbenchRemovedProjectWorkspaceCloser get removedProjectWorkspaceCloser;

  /// Tab subscriptions shared with the tab/layout owner.
  WorkbenchTabSubscriptionRegistry get tabSubscriptions;

  /// Serializes explicit tab closes and workspace sleeps per workspace.
  WorkbenchWorkspaceTabClosingScope get workspaceTabClosingScope;

  /// Workspaces whose persisted layout was cleared by a workspace sleep.
  WorkbenchClearedLayoutRegistry get clearedLayouts;

  /// Per-workspace MRU tab history used to pick a fallback active tab.
  WorkspaceTabFocusHistory get tabFocusHistory;

  /// Looks up a tracked workspace by id.
  Workspace? workspaceById(String workspaceId);

  /// Persists the current view preferences in the background.
  void persistViewPrefs();

  /// Drops worktree history entries that no longer resolve to a workspace.
  void pruneWorktreeNavigationHistory();

  /// Selects [workspace] through the selection owner.
  Future<void> selectCatalogWorkspace({
    required Project project,
    required Workspace workspace,
    required bool ensureInitialTerminal,
  });

  /// Makes [tabId] the active tab of [workspaceId] through the tab/layout
  /// owner.
  void activateWorkspaceTab({
    required String workspaceId,
    required String tabId,
    String? groupId,
  });

  /// Opens the deferred Setup tab through the tab/layout owner.
  Future<void> openDeferredSetupTab(WorkspaceCreationResult result);

  /// Loads the persisted layout of [workspaceId] through the tab/layout owner.
  Future<void> loadLayoutForWorkspace(String workspaceId);

  /// Reconciles the tracked tab set of [workspaceId] through the tab/layout
  /// owner.
  void handleWorkspaceTabsChanged(
    String workspaceId,
    List<WorkspaceTabRecord> tabs,
  );

  /// Ensures the selected workspace has a layout once tabs exist.
  void ensureSelectionHasTab();
}

/// Owns the catalog slice of the workbench: the project/workspace/section
/// read-model, watcher-driven sync of the tracked sets, and the catalog entry
/// points into `WorkspaceService` (create, rename, pin, sleep, delete).
final class WorkbenchCatalogOwner {
  WorkbenchCatalogOwner(this._host);

  final WorkbenchCatalogOwnerHost _host;

  final WorkbenchBootstrapGate _bootstrapGate = WorkbenchBootstrapGate();
  final WorkbenchRootSubscriptionRegistry _rootSubscriptions =
      WorkbenchRootSubscriptionRegistry();
  final WorkbenchWorkspaceSubscriptionRegistry _workspaceSubscriptions =
      WorkbenchWorkspaceSubscriptionRegistry();
  final WorkbenchWorktreeMetadataWatcherRegistry
  _worktreeMetadataWatcherRegistry = WorkbenchWorktreeMetadataWatcherRegistry();
  final WorkbenchMainWorkspacePreparationCoordinator _mainWorkspacePreparation =
      WorkbenchMainWorkspacePreparationCoordinator();
  final WorkbenchWorkspaceTagMutationQueue _workspaceTagMutations =
      WorkbenchWorkspaceTagMutationQueue();
  final WorkbenchWorkspaceParentMutationQueue _workspaceParentMutations =
      WorkbenchWorkspaceParentMutationQueue();
  final WorkbenchSerialMutationQueue _workspaceTreePinMutations =
      WorkbenchSerialMutationQueue();

  /// Cancels every watcher this owner started. Runs before the shared tab
  /// subscription registry so tab callbacks stop after workspace callbacks.
  void dispose() {
    _rootSubscriptions.cancelAll();
    _worktreeMetadataWatcherRegistry.disposeAll();
    _workspaceSubscriptions.cancelAll();
  }

  Future<void> bootstrap() {
    return _bootstrapGate.run(() async {
      final viewPrefsRepository = _host.viewPrefsRepository;
      final projectRepository = _host.projectsService.projectRepository;
      try {
        await const WorkbenchBootstrapOrchestrator().run(
          loadViewPrefs: () async {
            final repository = viewPrefsRepository;
            return repository == null ? null : await repository.load();
          },
          applyViewPrefs: (prefs) {
            if (!_host.isDisposed) {
              _host.emitState(_host.readState().copyWith(viewPrefs: prefs));
            }
          },
          watchViewPrefs: () {
            final repository = viewPrefsRepository;
            if (_host.isDisposed || repository == null) {
              return;
            }
            _rootSubscriptions.watchViewPrefs(
              repository.changes,
              onData: (prefs) {
                if (!_host.isDisposed) {
                  _host.emitState(_host.readState().copyWith(viewPrefs: prefs));
                }
              },
            );
          },
          startSections: () {
            if (!_host.isDisposed) {
              _startSections();
            }
          },
          watchProjects: () {
            if (_host.isDisposed) {
              return;
            }
            _rootSubscriptions.watchProjectsRecovering(
              projectRepository.watchAll,
              onData: _onProjectsChanged,
              onError: (Object _) {},
            );
          },
          listProjects: projectRepository.listAll,
          applyProjects: (projects) {
            if (!_host.isDisposed) {
              _onProjectsChanged(projects);
            }
          },
          ensureMainWorkspace: (project) => _host.isDisposed
              ? Future<void>.value()
              : _ensureMainWorkspaceForProject(project),
        );
        if (!_host.isDisposed) {
          _host.emitState(
            _host.readState().copyWith(bootstrapped: true, error: null),
          );
        }
      } catch (error) {
        if (!_host.isDisposed) {
          _host.emitState(
            _host.readState().copyWith(
              bootstrapped: true,
              error: 'Failed to bootstrap workbench: $error',
            ),
          );
        }
      }
    });
  }

  Future<T> _withWorktreeRefreshSuspended<T>(
    String projectId,
    Future<T> Function() action,
  ) {
    return _worktreeMetadataWatcherRegistry.withRefreshSuspended(
      projectId,
      action,
    );
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

  Future<void> _activateAddedProject(Project project) async {
    await _ensureMainWorkspaceForProject(project);
    final plan = planWorkbenchAddedProjectActivation(
      state: _host.readState(),
      project: project,
    );
    _host.emitState(plan.state);
    if (plan.viewPrefsChanged) {
      _host.persistViewPrefs();
    }
  }

  Future<void> _ensureMainWorkspaceForProject(Project project) {
    final workspaceService = _host.workspaceService;
    return _mainWorkspacePreparation.prepare(
      project: project,
      ensureMainWorkspace: workspaceService.ensureMainWorkspace,
      reconcile: workspaceService.reconcile,
      onError: (project, error) {
        if (!_host.isDisposed) {
          _host.emitState(
            _host.readState().copyWith(
              error:
                  'Failed to prepare workspace for "${project.name}": $error',
            ),
          );
        }
      },
    );
  }
}
