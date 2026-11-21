import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/application/global_agent_live_projection.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final now = DateTime.utc(2026, 11, 21);

  Project project(String id) => Project(
    id: id,
    name: id,
    repoPath: '/projects/$id',
    createdAt: now,
    updatedAt: now,
  );

  Workspace workspace(String id, String projectId) => Workspace(
    id: id,
    projectId: projectId,
    name: id,
    path: '/projects/$projectId/$id',
    createdAt: now,
    updatedAt: now,
    kind: WorkspaceKind.linked,
    status: WorkspaceStatus.active,
  );

  WorkspaceTabRecord tab(String id, String workspaceId) => WorkspaceTabRecord(
    id: id,
    workspaceId: workspaceId,
    kind: WorkspaceTabKind.terminal,
    title: 'Claude',
    createdAt: now,
    updatedAt: now,
    payload: <String, Object?>{
      workspaceTabTerminalSessionIdPayloadKey: 'session-$id',
    },
  );

  AgentStatusEntry status(String id, String workspaceId) => AgentStatusEntry(
    terminalSessionId: 'session-$id',
    workspaceId: workspaceId,
    tabId: id,
    agentType: AgentType.claude,
    state: AgentStatusState.working,
    prompt: 'Windows IME repair',
    updatedAt: now,
    stateStartedAt: now,
    contextOverview: <String, dynamic>{
      'goal': 'Fix Windows IME',
      'progress': 'Checking IMM API',
      'timeline': <Object>[],
    },
  );

  test('shows live sessions across every project, not just active project', () {
    final state = WorkbenchState(
      projects: <Project>[project('alera'), project('mcp')],
      workspacesByProject: <String, List<Workspace>>{
        'alera': <Workspace>[workspace('workspace-1', 'alera')],
        'mcp': <Workspace>[workspace('workspace-2', 'mcp')],
      },
      tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
        'workspace-1': <WorkspaceTabRecord>[tab('tab-1', 'workspace-1')],
        'workspace-2': <WorkspaceTabRecord>[tab('tab-2', 'workspace-2')],
      },
      activeProjectId: 'alera',
      activeWorkspaceId: 'workspace-1',
    );
    final rows = projectGlobalAgentLiveRows(state, <String, AgentStatusEntry>{
      'session-tab-1': status('tab-1', 'workspace-1'),
      'session-tab-2': status('tab-2', 'workspace-2'),
    });
    expect(rows, hasLength(2));
    expect(rows.map((row) => row.project).toSet(), <String>{'alera', 'mcp'});
    expect(rows.where((row) => row.matches('IMM API')), hasLength(2));
    expect(rows.where((row) => row.matches('no-such-context')), isEmpty);
  });

  test('rejects stale and foreign workspace-tab Host identity', () {
    final state = WorkbenchState(
      projects: <Project>[project('alera')],
      workspacesByProject: <String, List<Workspace>>{
        'alera': <Workspace>[workspace('workspace-1', 'alera')],
      },
      tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
        'workspace-1': <WorkspaceTabRecord>[tab('tab-1', 'workspace-1')],
      },
    );
    expect(
      projectGlobalAgentLiveRows(state, <String, AgentStatusEntry>{
        'session-tab-1': status('tab-1', 'wrong-workspace'),
      }),
      isEmpty,
    );
    expect(projectGlobalAgentLiveRows(state, const {}), isEmpty);
  });

  test(
    'guarded dashboard input requires current registered workspace and tab',
    () {
      final current = WorkbenchState(
        projects: <Project>[project('alera')],
        workspacesByProject: <String, List<Workspace>>{
          'alera': <Workspace>[workspace('worktree', 'alera')],
        },
        tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
          'worktree': <WorkspaceTabRecord>[tab('agent', 'worktree')],
        },
      );
      final statuses = <String, AgentStatusEntry>{
        'session-agent': status('agent', 'worktree'),
      };
      bool permitted(
        WorkbenchState state, {
        String workspaceId = 'worktree',
        String tabId = 'agent',
        String sessionId = 'session-agent',
      }) => globalAgentTargetIsCurrent(
        workbench: state,
        statuses: statuses,
        sessionId: sessionId,
        workspaceId: workspaceId,
        tabId: tabId,
      );
      expect(permitted(current), isTrue);
      expect(permitted(current, workspaceId: 'foreign'), isFalse);
      expect(permitted(current, tabId: 'recycled-tab'), isFalse);
      expect(permitted(current, sessionId: 'other-session'), isFalse);
      expect(
        permitted(
          WorkbenchState(
            projects: <Project>[project('alera')],
            tabsByWorkspace: current.tabsByWorkspace,
          ),
        ),
        isFalse,
      );
    },
  );
}
