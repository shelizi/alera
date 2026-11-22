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

  AgentStatusEntry status(
    String id,
    String workspaceId, {
    AgentStatusState state = AgentStatusState.working,
    bool? interrupted,
    List<Object?>? timeline,
  }) => AgentStatusEntry(
    terminalSessionId: 'session-$id',
    workspaceId: workspaceId,
    tabId: id,
    agentType: AgentType.claude,
    state: state,
    interrupted: interrupted,
    prompt: 'Windows IME repair',
    updatedAt: now,
    stateStartedAt: now,
    contextOverview: <String, dynamic>{
      'goal': 'Fix Windows IME',
      'progress': 'Checking IMM API',
      'timeline': timeline ?? <Object>[],
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

  test('Needs You queues only waiting/blocked verified agents with bounded timeline', () {
    final current = WorkbenchState(
      projects: <Project>[project('alera')],
      workspacesByProject: <String, List<Workspace>>{
        'alera': <Workspace>[workspace('worktree', 'alera')],
      },
      tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
        'worktree': <WorkspaceTabRecord>[
          for (final id in <String>[
            'wait',
            'blocked',
            'working',
            'done',
            'interrupted',
          ])
            tab(id, 'worktree'),
        ],
      },
    );
    final timeline = <Object?>[
      for (var i = 0; i < 19; i++)
        <String, Object?>{
          'kind': 'report',
          'text': 'Report $i',
          'at': DateTime.utc(2026, 11, 21, 12, i).toIso8601String(),
        },
      <String, Object?>{
        'kind': 'toolInput',
        'text': 'Never expose tool payloads',
        'at': now.toIso8601String(),
      },
    ];
    final rows = projectGlobalAgentLiveRows(current, <String, AgentStatusEntry>{
      'session-wait': status(
        'wait',
        'worktree',
        state: AgentStatusState.waiting,
        timeline: timeline,
      ),
      'session-blocked': status(
        'blocked',
        'worktree',
        state: AgentStatusState.blocked,
      ),
      'session-working': status('working', 'worktree'),
      'session-done': status('done', 'worktree', state: AgentStatusState.done),
      'session-interrupted': status(
        'interrupted',
        'worktree',
        state: AgentStatusState.done,
        interrupted: true,
      ),
    });
    final needsReview = rows.where((row) => row.needsReview).toList();
    expect(needsReview.map((row) => row.tab.id).toSet(), <String>{
      'wait',
      'blocked',
    });
    final waiting = rows.singleWhere((row) => row.tab.id == 'wait');
    expect(waiting.timeline, hasLength(16));
    expect(waiting.timeline.first.text, 'Report 15');
    expect(waiting.timeline.last.text, 'Report 0');
    expect(waiting.timeline.every((event) => event.kind == 'report'), isTrue);
    expect(
      waiting.timeline.any((event) => event.text.contains('tool')),
      isFalse,
    );
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
