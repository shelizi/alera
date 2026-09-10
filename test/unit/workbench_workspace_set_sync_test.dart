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

  test('applies workspace sync plan while preserving unrelated state', () {
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
      searchQuery: 'keep-search',
      error: 'keep-error',
    );
    final workspaces = <Workspace>[liveWorkspace];
    final plan = planWorkbenchWorkspaceSetSync(
      state: state,
      project: project,
      workspaces: workspaces,
    );

    final next = applyWorkbenchWorkspaceSetSyncPlan(state: state, plan: plan);

    expect(next.projects, same(state.projects));
    expect(next.workspacesByProject, plan.workspacesByProject);
    expect(next.layoutByWorkspace, plan.layoutByWorkspace);
    expect(next.tabsByWorkspace, plan.tabsByWorkspace);
    expect(next.activeTabIdByWorkspace, plan.activeTabIdByWorkspace);
    expect(next.viewPrefs, plan.viewPrefs);
    expect(next.activeProjectId, plan.activeProjectId);
    expect(next.activeWorkspaceId, plan.activeWorkspaceId);
    expect(next.searchQuery, 'keep-search');
    expect(next.error, 'keep-error');
  });

  test(
    'reconciles a missing workspace into only its project workspace list',
    () {
      final now = DateTime.utc(2026, 9, 10);
      final project = _project('project', now);
      final otherProject = _project('other-project', now);
      final existing = _workspace('existing', project.id, now);
      final added = _workspace('added', project.id, now);
      final otherWorkspace = _workspace('other', otherProject.id, now);
      final state = WorkbenchState(
        projects: <Project>[project, otherProject],
        workspacesByProject: <String, List<Workspace>>{
          project.id: <Workspace>[existing],
          otherProject.id: <Workspace>[otherWorkspace],
        },
        activeProjectId: otherProject.id,
        activeWorkspaceId: otherWorkspace.id,
        searchQuery: 'keep-search',
      );

      final next = reconcileWorkbenchWorkspaceState(
        state: state,
        projectId: project.id,
        workspace: added,
      );

      expect(next.workspacesFor(project.id), <Workspace>[existing, added]);
      expect(next.workspacesFor(otherProject.id), <Workspace>[otherWorkspace]);
      expect(next.activeProjectId, otherProject.id);
      expect(next.activeWorkspaceId, otherWorkspace.id);
      expect(next.searchQuery, 'keep-search');
    },
  );

  test('reconciles an existing workspace in place without duplicating it', () {
    final now = DateTime.utc(2026, 9, 10);
    final project = _project('project', now);
    final existing = _workspace('workspace', project.id, now);
    final sibling = _workspace('sibling', project.id, now);
    final updated = Workspace(
      id: existing.id,
      projectId: project.id,
      name: 'Updated workspace',
      path: 'C:/updated',
      createdAt: now,
      updatedAt: now.add(const Duration(minutes: 1)),
      kind: WorkspaceKind.main,
      status: WorkspaceStatus.active,
    );
    final state = WorkbenchState(
      projects: <Project>[project],
      workspacesByProject: <String, List<Workspace>>{
        project.id: <Workspace>[existing, sibling],
      },
    );

    final next = reconcileWorkbenchWorkspaceState(
      state: state,
      projectId: project.id,
      workspace: updated,
    );

    expect(next.workspacesFor(project.id), <Workspace>[updated, sibling]);
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
