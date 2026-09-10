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
  test(
    'applies project sync plan to state while preserving unrelated fields',
    () {
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
        activeTabIdByWorkspace: <String, String>{
          liveWorkspace.id: 'live-tab',
          removedWorkspace.id: 'removed-tab',
        },
        searchQuery: 'keep-search',
        error: 'keep-error',
      );
      final projects = <Project>[liveProject];
      final plan = planWorkbenchProjectSetSync(
        state: state,
        projects: projects,
      );

      final next = applyWorkbenchProjectSetSyncPlan(
        state: state,
        projects: projects,
        plan: plan,
      );

      expect(next.projects, projects);
      expect(next.workspacesByProject, plan.workspacesByProject);
      expect(next.tabsByWorkspace, plan.tabsByWorkspace);
      expect(next.layoutByWorkspace, plan.layoutByWorkspace);
      expect(next.activeTabIdByWorkspace, plan.activeTabIdByWorkspace);
      expect(next.viewPrefs, plan.viewPrefs);
      expect(next.activeProjectId, plan.activeProjectId);
      expect(next.activeWorkspaceId, plan.activeWorkspaceId);
      expect(next.searchQuery, 'keep-search');
      expect(next.error, 'keep-error');
    },
  );

  test(
    'project update replaces in place and preserves unrelated workbench state',
    () {
      final now = DateTime.utc(2026, 9, 10);
      final first = _project('first', now);
      final target = _project('target', now);
      final updated = Project(
        id: target.id,
        name: 'Renamed target',
        repoPath: target.repoPath,
        createdAt: target.createdAt,
        updatedAt: now.add(const Duration(minutes: 1)),
      );
      final workspace = _workspace('target-ws', target.id, now);
      final prefs = WorkbenchViewPrefs.defaults.copyWith(
        selectedProjectIds: <String>{target.id},
      );
      final state = WorkbenchState(
        projects: <Project>[first, target],
        workspacesByProject: <String, List<Workspace>>{
          target.id: <Workspace>[workspace],
        },
        activeProjectId: target.id,
        activeWorkspaceId: workspace.id,
        viewPrefs: prefs,
        searchQuery: 'keep-search',
        error: 'stale error',
      );

      final next = applyWorkbenchProjectUpdateState(
        state: state,
        project: updated,
      );

      expect(next.projects, <Project>[first, updated]);
      expect(next.activeProjectId, target.id);
      expect(next.activeWorkspaceId, workspace.id);
      expect(next.workspacesByProject, same(state.workspacesByProject));
      expect(next.viewPrefs, same(prefs));
      expect(next.searchQuery, 'keep-search');
      expect(next.error, isNull);
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
