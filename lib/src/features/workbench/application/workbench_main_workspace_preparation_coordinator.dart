import 'package:alera/src/features/projects/domain/project.dart';

final class WorkbenchMainWorkspacePreparationCoordinator {
  final Set<String> _preparingProjectIds = <String>{};

  Future<void> prepare({
    required Project project,
    required Future<void> Function(Project project) ensureMainWorkspace,
    required Future<void> Function(Project project) reconcile,
    required void Function(Project project, Object error) onError,
  }) async {
    if (!_preparingProjectIds.add(project.id)) {
      return;
    }
    try {
      await ensureMainWorkspace(project);
      await reconcile(project);
    } catch (error) {
      onError(project, error);
    } finally {
      _preparingProjectIds.remove(project.id);
    }
  }
}
