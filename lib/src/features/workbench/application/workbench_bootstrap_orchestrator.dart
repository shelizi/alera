import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';

final class WorkbenchBootstrapOrchestrator {
  const WorkbenchBootstrapOrchestrator();

  Future<void> run({
    required Future<WorkbenchViewPrefs?> Function() loadViewPrefs,
    required void Function(WorkbenchViewPrefs prefs) applyViewPrefs,
    required void Function() watchViewPrefs,
    required void Function() startSections,
    required void Function() watchProjects,
    required Future<List<Project>> Function() listProjects,
    required void Function(List<Project> projects) applyProjects,
    required Future<void> Function(Project project) ensureMainWorkspace,
  }) async {
    try {
      final prefs = await loadViewPrefs();
      if (prefs != null) {
        applyViewPrefs(prefs);
        watchViewPrefs();
      }
    } catch (_) {
      // Persisted view preferences are optional bootstrap state. Keep the
      // established behavior of falling back to defaults when this phase fails.
    }

    startSections();
    watchProjects();
    final projects = await listProjects();
    applyProjects(projects);
    await Future.wait<void>(projects.map(ensureMainWorkspace));
  }
}
