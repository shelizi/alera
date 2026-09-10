import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/worktree_navigation_history.dart';

final class WorkbenchNavigationSelection {
  const WorkbenchNavigationSelection({
    required this.target,
    required this.project,
    required this.workspace,
  });

  final WorktreeNavigationTarget target;
  final Project project;
  final Workspace workspace;
}

final class WorkbenchNavigationHistoryService {
  WorkbenchNavigationHistoryService({WorktreeNavigationHistory? history})
    : _history = history ?? WorktreeNavigationHistory();

  final WorktreeNavigationHistory _history;

  bool canGoBack(WorkbenchState state) {
    prune(state);
    return _history.canGoBack;
  }

  bool canGoForward(WorkbenchState state) {
    prune(state);
    return _history.canGoForward;
  }

  bool record({required Project project, required Workspace workspace}) {
    return _history.record(
      WorktreeNavigationTarget(
        projectId: project.id,
        workspaceId: workspace.id,
      ),
    );
  }

  WorkbenchNavigationSelection? peekBack(WorkbenchState state) {
    final target = _history.peekBack(
      isValid: (target) => _selectionFor(state, target) != null,
    );
    return target == null ? null : _selectionFor(state, target);
  }

  WorkbenchNavigationSelection? peekForward(WorkbenchState state) {
    final target = _history.peekForward(
      isValid: (target) => _selectionFor(state, target) != null,
    );
    return target == null ? null : _selectionFor(state, target);
  }

  void commitBack(WorkbenchNavigationSelection selection) {
    _history.commitBack(selection.target);
  }

  void commitForward(WorkbenchNavigationSelection selection) {
    _history.commitForward(selection.target);
  }

  void prune(WorkbenchState state) {
    _history.prune((target) => _selectionFor(state, target) != null);
  }

  WorkbenchNavigationSelection? _selectionFor(
    WorkbenchState state,
    WorktreeNavigationTarget target,
  ) {
    Project? project;
    for (final candidate in state.projects) {
      if (candidate.id == target.projectId) {
        project = candidate;
        break;
      }
    }
    if (project == null) {
      return null;
    }

    Workspace? workspace;
    for (final candidate in state.workspacesFor(project.id)) {
      if (candidate.id == target.workspaceId) {
        workspace = candidate;
        break;
      }
    }
    if (workspace == null) {
      return null;
    }

    return WorkbenchNavigationSelection(
      target: target,
      project: project,
      workspace: workspace,
    );
  }
}
