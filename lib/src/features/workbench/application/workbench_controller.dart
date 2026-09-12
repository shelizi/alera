import 'package:alera/src/features/workbench/domain/workspace_section.dart';

import 'dart:async';

import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/application/agent_status_providers.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/projects/application/project_providers.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/projects/application/projects_service.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workspace_explorer_reveal.dart';
import 'package:alera/src/features/workbench/application/workspace_tab_service.dart';
import 'package:alera/src/features/workbench/application/workbench_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/application/workbench_listing.dart';
import 'package:alera/src/features/workbench/application/workbench_layout_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_retired_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_cleared_layout_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_retired_tabs_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_retired_workspace_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_remove_project_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_explicit_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_git_repository_probe_adapter.dart';
import 'package:alera/src/features/workbench/application/workbench_source_control_folder_focus_service.dart';
import 'package:alera/src/features/workbench/application/workbench_catalog_owner.dart';
import 'package:alera/src/features/workbench/application/workbench_replaceable_tab_editor_sessions.dart';
import 'package:alera/src/features/workbench/application/workbench_selection_owner.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_layout_owner.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tab_closing_scope.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_selection_hydrator.dart';
import 'package:alera/src/features/workbench/application/workbench_source_control_root_prefs.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_view_pref_expansion.dart';
import 'package:alera/src/features/workbench/application/workbench_view_pref_filters.dart';
import 'package:alera/src/features/workbench/application/workbench_view_prefs_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_view_prefs_persistence_queue.dart';
import 'package:alera/src/features/workbench/application/workspace_activity_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';
import 'package:alera/src/features/workbench/application/workspace_service.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

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
        _WorkbenchControllerSections {
  @override
  WorkbenchState build() {
    _disposed = false;
    ref.onDispose(() {
      _disposed = true;
      _catalogOwner.dispose();
      _tabSubscriptions.cancelAll();
    });
    return const WorkbenchState();
  }

  Future<void> bootstrap() => _catalogOwner.bootstrap();
}
