import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_set_sync.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('prunes state owned by workspaces that disappeared', () {
    final now = DateTime.utc(2026, 9, 10);
    final project = _project('project', now);
    final liveWorkspace = _workspace('live-ws', project.id, now);
    final removedWorkspace = _workspace('removed-ws', project.id, now);
    final state = WorkbenchState(
      projects: <Project>[project],
      activeProjectId: project.id,
      activeWorkspaceId: removedWorkspace.id,
      workspacesByProject: <String, List<Workspace>>{
        project.id: <Workspace>[liveWorkspace, removedWorkspace],
      },
      activeTabIdByWorkspace: <String, String>{
        liveWorkspace.id: 'live-tab',
        removedWorkspace.id: 'removed-tab',
      },
      viewPrefs: WorkbenchViewPrefs.defaults.copyWith(
        expandedWorkspaceIds: <String>{liveWorkspace.id, removedWorkspace.id},
        sourceControlRootByWorkspaceId: <String, String>{
          liveWorkspace.id: 'packages/live',
          removedWorkspace.id: 'packages/removed',
        },
      ),
    );

    final plan = planWorkbenchWorkspaceSetSync(
      state: state,
      project: project,
      workspaces: <Workspace>[liveWorkspace],
    );

    expect(plan.removedWorkspaceIds, <String>{removedWorkspace.id});
    expect(plan.workspacesByProject[project.id], <Workspace>[liveWorkspace]);
    expect(plan.activeWorkspaceId, isNull);
    expect(plan.activeTabIdByWorkspace, <String, String>{
      liveWorkspace.id: 'live-tab',
    });
    expect(plan.viewPrefs.expandedWorkspaceIds, <String>{liveWorkspace.id});
    expect(plan.viewPrefs.sourceControlRootByWorkspaceId, <String, String>{
      liveWorkspace.id: 'packages/live',
    });
    expect(plan.viewPrefsChanged, isTrue);
  });
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
