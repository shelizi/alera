import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_git_auto_fetch_scheduler.dart';
import 'package:alera/src/features/workbench/application/workspace_source_control_controller.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'workbench_git_auto_fetch.g.dart';

/// Owned by the primary shell, not by Git panel visibility. Stopping the
/// provider (or disabling the setting) cancels subsequent scheduled runs.
@Riverpod(keepAlive: true)
GitAutoFetchScheduler workbenchGitAutoFetchCoordinator(Ref ref) {
  final logger = Logger('GitAutoFetch');
  final scheduler = GitAutoFetchScheduler(
    fetch: ref.read(gitBackendProvider).fetch,
    onFetched: (path) {
      final workbench = ref.read(workbenchControllerProvider);
      final project = workbench.activeProject;
      if (project == null ||
          !gitAutoFetchRepositoryPaths(<Project>[project]).contains(path)) {
        return;
      }
      final scope = WorkspaceSourceControlScope.resolve(
        project: project,
        workspace: workbench.activeWorkspace,
        prefs: workbench.viewPrefs,
      );
      if (scope != null) {
        // Invalidate an already-listened source-control controller to refresh
        // ahead/behind status. Inactive controllers are not instantiated.
        ref.invalidate(workspaceSourceControlControllerProvider(scope.path));
      }
    },
    onError: (path, error, stackTrace) => logger.warning(
      'background git fetch failed for $path',
      error,
      stackTrace,
    ),
  );

  void configure() {
    final settings = ref.read(settingsControllerProvider);
    final workbench = ref.read(workbenchControllerProvider);
    scheduler.configure(
      enabled: workbench.bootstrapped && settings.general.autoGitFetchEnabled,
      paths: gitAutoFetchRepositoryPaths(workbench.projects),
      interval: Duration(
        minutes: settings.general.autoGitFetchIntervalMinutes.clamp(1, 60),
      ),
    );
  }

  ref.listen(
    settingsControllerProvider.select(
      (settings) => (
        settings.general.autoGitFetchEnabled,
        settings.general.autoGitFetchIntervalMinutes,
      ),
    ),
    (_, __) => configure(),
  );
  ref.listen(
    workbenchControllerProvider.select(
      (state) => (state.bootstrapped, state.projects),
    ),
    (_, __) => configure(),
  );
  configure();
  ref.onDispose(scheduler.dispose);
  return scheduler;
}
