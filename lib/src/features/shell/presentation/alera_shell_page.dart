import 'dart:async';
import 'dart:io';

import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/features/workbench/application/branch_compare_current_service.dart';
import 'package:alera/src/features/shell/application/branch_merge_into_current_service.dart';
import 'package:alera/src/features/workbench/application/workbench_branch_favorites.dart';
import 'package:alera/src/features/workbench/application/workspace_explorer_reveal.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/application/detached_tab_window_service.dart';

import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_status/application/runtime_agent_status_sync.dart';
import 'package:alera/src/features/agent_status/application/agent_status_host_forwarder.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/agent_quota/presentation/agent_quota_status_bar.dart';
import 'package:alera/src/features/keep_alive/application/keep_alive_providers.dart';
import 'package:alera/src/features/keep_alive/presentation/keep_alive_status_bar.dart';
import 'package:alera/src/features/language_intelligence/presentation/language_intelligence_status_bar.dart';
import 'package:alera/src/features/runtime_host/presentation/runtime_host_status_bar.dart';
import 'package:alera/src/features/resource_manager/presentation/resource_status_bar_control.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/features/app_menu/presentation/alera_app_menu_scope.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/application/external_editor_providers.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launch_result.dart';
import 'package:alera/src/features/keyboard/presentation/keyboard_shortcuts_scope.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/pull_requests/application/workspace_pull_request_monitor_providers.dart';
import 'package:alera/src/features/pull_requests/application/workspace_pull_request_notification_providers.dart';
import 'package:alera/src/features/workbench/application/workbench_archive_sweep.dart';
import 'package:alera/src/features/workbench/application/workbench_git_auto_fetch.dart';
import 'package:alera/src/features/workbench/application/workbench_git_repository_lifecycle_monitor.dart';
import 'package:alera/src/features/workbench/application/workspace_language_intelligence_prewarm_coordinator.dart';
import 'package:alera/src/features/workbench/application/workspace_source_control_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/application/workspace_file_open_coordinator_provider.dart';
import 'package:alera/src/features/workbench/application/external_terminal_providers.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_acknowledgements.dart';
import 'package:alera/src/features/workbench/domain/external_terminal_launcher.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:alera/src/shared/infra/runtime/runtime_host_providers.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/features/workbench/presentation/workspace_context_sidebar.dart';
import 'package:alera/src/features/workbench/presentation/workbench_dialog_launchers.dart';
import 'package:alera/src/features/workbench/presentation/project_workbench_sidebar.dart';
import 'package:alera/src/features/workbench/presentation/welcome_dashboard.dart';
import 'package:alera/src/features/workbench/application/terminal_driver_presence_controller.dart';
import 'package:alera/src/features/workbench/presentation/mobile_driver_overlay.dart';
import 'package:alera/src/features/workbench/presentation/workbench_close_confirmation.dart';
import 'package:alera/src/features/workbench/presentation/workspace_workbench_view.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:alera/src/shared/infra/git/git_remote.dart';
import 'package:alera/src/features/settings/presentation/github_star_prompt_watch.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

part 'alera_shell_page_body.dart';
part 'workspace_branch_status_bar_control.dart';
part 'workspace_branch_tree_projection.dart';
part 'alera_shell_page_body_content.dart';
part 'alera_shell_page_mcp_ui.dart';
part 'alera_shell_page_mcp_path.dart';

/// The production resolver canonicalizes real files and enforces Workspace
/// boundaries. Injection allows Shell tests to verify UI/Host behavior without
/// using async filesystem I/O inside Flutter's fake-async widget clock.
typedef McpWorkspaceFilePathResolver = Future<String> Function(
  Workspace workspace,
  Object? rawPath,
);

final mcpWorkspaceFilePathResolverProvider =
    Provider<McpWorkspaceFilePathResolver>(
      (ref) => mcpResolveExistingWorkspacePath,
    );

class const AleraShellPage({super.key}) extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final dbAsync = ref.watch(aleraDatabaseProvider);
    return dbAsync.when(
      loading: () => const _ShellLoading(),
      error: (error, _) => _ShellError(error: error.toString()),
      data: (_) => const GitHubStarPromptWatch(child: _AleraShellPageBody()),
    );
  }
}

class const _ShellLoading() extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return const Scaffold(body: Center(child: CircularProgressIndicator()));
  }
}

class const _ShellError({required final String error}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      body: Center(
        child: Padding(
          padding: const EdgeInsets.all(AleraTokens.space24),
          child: Column(
            mainAxisSize: .min,
            children: <Widget>[
              const Icon(AleraIcons.error, color: AleraTokens.error, size: 32),
              const SizedBox(height: AleraTokens.space12),
              Text(
                'Failed to open the local database',
                style: theme.textTheme.titleMedium,
              ),
              const SizedBox(height: AleraTokens.space8),
              Text(
                error,
                textAlign: .center,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: AleraTokens.foregroundMuted,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class const _AleraShellPageBody() extends ConsumerStatefulWidget {
  @override
  ConsumerState<_AleraShellPageBody> createState() =>
      _AleraShellPageBodyState();
}

double _maxContextSidebarWidth({
  required double shellWidth,
  required bool collapsed,
  required WorkbenchViewPrefs prefs,
}) {
  final leftWidth = collapsed
      ? AleraTokens.sidebarCollapsedWidth
      : prefs.sidebarWidth;
  // Reserve space for the editor before sizing the context sidebar. In
  // particular, do not hide the panel after the user enlarges it.
  return (shellWidth -
          leftWidth -
          AleraTokens.rightSidebarMinEditorWidth -
          AleraTokens.space6)
      .clamp(0.0, AleraTokens.rightSidebarMaxWidth);
}

String buildRawLogClipboardText(List<String> logs) {
  return logs.join('\n');
}
