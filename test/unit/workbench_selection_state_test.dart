import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_selection_state.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'selecting a workspace updates selection and clears the prior error',
    () {
      final project = _project('project');
      final workspace = _workspace('workspace', project.id);
      final prefs = WorkbenchViewPrefs.defaults.copyWith(
        rightSidebarVisible: false,
      );
      final state = WorkbenchState(
        projects: <Project>[project],
        workspacesByProject: <String, List<Workspace>>{
          project.id: <Workspace>[workspace],
        },
        viewPrefs: prefs,
        activeProjectId: 'old-project',
        activeWorkspaceId: 'old-workspace',
        searchQuery: 'keep-search',
        error: 'old error',
      );

      final next = selectWorkbenchWorkspace(
        state: state,
        project: project,
        workspace: workspace,
      );

      expect(next.activeProjectId, project.id);
      expect(next.activeWorkspaceId, workspace.id);
      expect(next.error, isNull);
      expect(next.viewPrefs, same(prefs));
      expect(next.searchQuery, 'keep-search');
      expect(next.projects, same(state.projects));
      expect(next.workspacesByProject, same(state.workspacesByProject));
    },
  );

  test(
    'activating a project clears only the workspace selection and error',
    () {
      final project = _project('project');
      final prefs = WorkbenchViewPrefs.defaults.copyWith(
        rightSidebarVisible: false,
      );
      final state = WorkbenchState(
        projects: <Project>[project],
        viewPrefs: prefs,
        activeProjectId: 'old-project',
        activeWorkspaceId: 'old-workspace',
        searchQuery: 'keep-search',
        error: 'old error',
      );

      final next = activateWorkbenchProject(state: state, project: project);

      expect(next.activeProjectId, project.id);
      expect(next.activeWorkspaceId, isNull);
      expect(next.error, isNull);
      expect(next.viewPrefs, same(prefs));
      expect(next.searchQuery, 'keep-search');
    },
  );
  test('added project activation expands stale collapsed project prefs', () {
    final project = _project('project');
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      collapsedProjectIds: <String>{project.id, 'other'},
      selectedProjectIds: const <String>{'filtered'},
    );
    final state = WorkbenchState(
      viewPrefs: prefs,
      activeProjectId: 'old-project',
      activeWorkspaceId: 'old-workspace',
      searchQuery: 'keep-search',
      error: 'old error',
    );

    final plan = planWorkbenchAddedProjectActivation(
      state: state,
      project: project,
    );

    expect(plan.viewPrefsChanged, isTrue);
    expect(plan.state.activeProjectId, project.id);
    expect(plan.state.activeWorkspaceId, isNull);
    expect(plan.state.error, isNull);
    expect(plan.state.searchQuery, 'keep-search');
    expect(plan.state.viewPrefs.collapsedProjectIds, <String>{'other'});
    expect(
      plan.state.viewPrefs.selectedProjectIds,
      same(prefs.selectedProjectIds),
    );
  });

  test(
    'added project activation preserves prefs identity when already expanded',
    () {
      final project = _project('project');
      final prefs = WorkbenchViewPrefs.defaults.copyWith(
        collapsedProjectIds: const <String>{'other'},
      );
      final state = WorkbenchState(
        viewPrefs: prefs,
        activeProjectId: 'old-project',
        activeWorkspaceId: 'old-workspace',
        error: 'old error',
      );

      final plan = planWorkbenchAddedProjectActivation(
        state: state,
        project: project,
      );

      expect(plan.viewPrefsChanged, isFalse);
      expect(plan.state.viewPrefs, same(prefs));
      expect(plan.state.activeProjectId, project.id);
      expect(plan.state.activeWorkspaceId, isNull);
      expect(plan.state.error, isNull);
    },
  );
}

Project _project(String id) {
  final now = DateTime.utc(2026, 9, 10);
  return Project(
    id: id,
    name: id,
    repoPath: 'C:/$id',
    createdAt: now,
    updatedAt: now,
  );
}

Workspace _workspace(String id, String projectId) {
  final now = DateTime.utc(2026, 9, 10);
  return Workspace(
    id: id,
    projectId: projectId,
    name: id,
    path: 'C:/$id',
    createdAt: now,
    updatedAt: now,
    kind: WorkspaceKind.main,
    status: WorkspaceStatus.active,
  );
}
