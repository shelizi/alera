import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_project_set_sync.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('prunes state owned by projects that disappeared', () {
    final now = DateTime.utc(2026, 9, 10);
    final liveProject = _project('live', now);
    final removedProject = _project('removed', now);
    final liveWorkspace = _workspace('live-ws', liveProject.id, now);
    final removedWorkspace = _workspace('removed-ws', removedProject.id, now);
    final state = WorkbenchState(
      projects: <Project>[liveProject, removedProject],
      activeProjectId: removedProject.id,
      activeWorkspaceId: removedWorkspace.id,
      workspacesByProject: <String, List<Workspace>>{
        liveProject.id: <Workspace>[liveWorkspace],
        removedProject.id: <Workspace>[removedWorkspace],
      },
      tabsByWorkspace: const <String, List<WorkspaceTabRecord>>{},
      activeTabIdByWorkspace: <String, String>{
        liveWorkspace.id: 'live-tab',
        removedWorkspace.id: 'removed-tab',
      },
      viewPrefs: WorkbenchViewPrefs.defaults.copyWith(
        collapsedProjectIds: <String>{liveProject.id, removedProject.id},
        selectedProjectIds: <String>{liveProject.id, removedProject.id},
        sourceControlRootByWorkspaceId: <String, String>{
          liveWorkspace.id: 'packages/live',
          removedWorkspace.id: 'packages/removed',
        },
      ),
    );

    final plan = planWorkbenchProjectSetSync(
      state: state,
      projects: <Project>[liveProject],
    );

    expect(plan.validProjectIds, <String>{liveProject.id});
    expect(plan.removedWorkspaces, <Workspace>[removedWorkspace]);
    expect(plan.activeProjectId, liveProject.id);
    expect(plan.activeWorkspaceId, isNull);
    expect(plan.workspacesByProject.keys, <String>[liveProject.id]);
    expect(plan.activeTabIdByWorkspace, <String, String>{
      liveWorkspace.id: 'live-tab',
    });
    expect(plan.viewPrefs.collapsedProjectIds, <String>{liveProject.id});
    expect(plan.viewPrefs.selectedProjectIds, <String>{liveProject.id});
    expect(plan.viewPrefs.sourceControlRootByWorkspaceId, <String, String>{
      liveWorkspace.id: 'packages/live',
    });
    expect(plan.viewPrefsChanged, isTrue);
  });

  test(
    'keeps an active workspace that still belongs to the active project',
    () {
      final now = DateTime.utc(2026, 9, 10);
      final project = _project('live', now);
      final workspace = _workspace('live-ws', project.id, now);
      final state = WorkbenchState(
        projects: <Project>[project],
        workspacesByProject: <String, List<Workspace>>{
          project.id: <Workspace>[workspace],
        },
        activeProjectId: project.id,
        activeWorkspaceId: workspace.id,
      );

      final plan = planWorkbenchProjectSetSync(
        state: state,
        projects: <Project>[project],
      );

      expect(plan.activeProjectId, project.id);
      expect(plan.activeWorkspaceId, workspace.id);
    },
  );
}

Project _project(String id, DateTime now) => Project(
  id: id,
  name: id,
  repoPath: 'C:/$id',
  createdAt: now,
  updatedAt: now,
);

Workspace _workspace(String id, String projectId, DateTime now) => Workspace(
  id: id,
  projectId: projectId,
  name: id,
  path: 'C:/$id',
  createdAt: now,
  updatedAt: now,
  kind: WorkspaceKind.main,
  status: WorkspaceStatus.active,
);
