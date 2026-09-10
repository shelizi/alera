import 'package:alera/src/features/workbench/domain/workspace_section.dart';
import 'package:alera/src/features/workbench/application/workspace_section_repository.dart';

import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/application/agent_status_providers.dart';
import 'package:alera/src/features/projects/application/project_providers.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/projects/application/projects_service.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workspace_explorer_reveal.dart';
import 'package:alera/src/features/workbench/application/workspace_tab_service.dart';
import 'package:alera/src/features/workbench/application/workbench_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/application/workbench_listing.dart';
import 'package:alera/src/features/workbench/application/workbench_navigation_history_service.dart';
import 'package:alera/src/features/workbench/application/workbench_retired_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_explicit_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_file_tab_mutation_queue.dart';
import 'package:alera/src/features/workbench/application/workbench_file_preview_policy.dart';
import 'package:alera/src/features/workbench/application/workbench_file_tab_open_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_file_tab_path_move_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_bootstrap_orchestrator.dart';
import 'package:alera/src/features/workbench/application/workbench_bootstrap_gate.dart';
import 'package:alera/src/features/workbench/application/workbench_main_workspace_preparation_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_layout_load_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_layout_resolver.dart';
import 'package:alera/src/features/workbench/application/workbench_layout_state.dart';
import 'package:alera/src/features/workbench/application/workbench_cleared_layout_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_closed_tabs_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_project_set_sync.dart';
import 'package:alera/src/features/workbench/application/workbench_root_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_section_state.dart';
import 'package:alera/src/features/workbench/application/workbench_selection_state.dart';
import 'package:alera/src/features/workbench/application/workbench_sleep_workspace_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_removal_state.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_set_sync.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_placement_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tag_update_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_parent_update_service.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_creation_parent_link_service.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tree_pin_policy.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_set_sync.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tab_closing_scope.dart';
import 'package:alera/src/features/workbench/application/workspace_descendants.dart';
import 'package:alera/src/features/workbench/application/workbench_source_control_root_prefs.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_view_pref_expansion.dart';
import 'package:alera/src/features/workbench/application/workbench_view_pref_filters.dart';
import 'package:alera/src/features/workbench/application/workbench_view_prefs_repository.dart';
import 'package:alera/src/features/workbench/application/workspace_activity_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_file_preview_kind.dart';
import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_worktree_metadata_watcher_registry.dart';
import 'package:alera/src/features/workbench/application/workspace_service.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:path/path.dart' as p;
import 'package:riverpod_annotation/riverpod_annotation.dart';
import 'package:uuid/uuid.dart';

part 'workbench_controller.g.dart';
part 'workbench_controller_internals.dart';
part 'workbench_controller_projects.dart';
part 'workbench_controller_navigation.dart';
part 'workbench_controller_tab_opening.dart';
part 'workbench_controller_file_tabs.dart';
part 'workbench_controller_pull_request_diff_tabs.dart';
part 'workbench_controller_workspace_creation.dart';
part 'workbench_controller_tabs.dart';
part 'workbench_controller_view_prefs.dart';
part 'workbench_controller_sync.dart';
part 'workbench_controller_sections.dart';

@Riverpod(keepAlive: true)
class WorkbenchController extends _$WorkbenchController
    with
        _WorkbenchControllerInternals,
        _WorkbenchControllerTabOpening,
        _WorkbenchControllerFileTabs,
        _WorkbenchControllerPullRequestDiffTabs,
        _WorkbenchControllerProjects,
        _WorkbenchControllerNavigation,
        // Creation builds on project selection and tab opening so the prompt
        // flow can synchronize its agent before appending Setup.
        _WorkbenchControllerWorkspaceCreation,
        _WorkbenchControllerTabs,
        _WorkbenchControllerViewPrefs,
        _WorkbenchControllerSync,
        _WorkbenchControllerSections {
  @override
  WorkbenchState build() {
    _disposed = false;
    ref.onDispose(() {
      _disposed = true;
      _rootSubscriptions.cancelAll();
      _worktreeMetadataWatcherRegistry.disposeAll();
      _workspaceSubscriptions.cancelAll();
      _tabSubscriptions.cancelAll();
    });
    return const WorkbenchState();
  }

  Future<void> bootstrap() {
    return _bootstrapGate.run(() async {
      final viewPrefsRepository = _viewPrefsRepository;
      final projectRepository = _projectsService.projectRepository;
      try {
        await const WorkbenchBootstrapOrchestrator().run(
          loadViewPrefs: () async {
            final repository = viewPrefsRepository;
            return repository == null ? null : await repository.load();
          },
          applyViewPrefs: (prefs) {
            state = state.copyWith(viewPrefs: prefs);
          },
          watchViewPrefs: () {
            final repository = viewPrefsRepository;
            if (repository == null) {
              return;
            }
            _rootSubscriptions.watchViewPrefs(
              repository.changes,
              onData: (prefs) {
                if (!_disposed) state = state.copyWith(viewPrefs: prefs);
              },
            );
          },
          startSections: _startSections,
          watchProjects: () {
            _rootSubscriptions.watchProjectsRecovering(
              projectRepository.watchAll,
              onData: _onProjectsChanged,
              onError: (Object _) {},
            );
          },
          listProjects: projectRepository.listAll,
          applyProjects: _onProjectsChanged,
          ensureMainWorkspace: _ensureMainWorkspaceForProject,
        );
        state = state.copyWith(bootstrapped: true, error: null);
      } catch (error) {
        state = state.copyWith(
          bootstrapped: true,
          error: 'Failed to bootstrap workbench: $error',
        );
      }
    });
  }
}
