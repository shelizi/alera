import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_listing.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workspace_agent_run_groups.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

final DateTime _t0 = .utc(2026, 5, 1);

void main() {
  test(
    'workspace glyph prefers a later working agent over an earlier done one',
    () {
      final project = Project(
        id: 'p-alera',
        name: 'alera',
        repoPath: '/repo/p-alera',
        createdAt: _t0,
        updatedAt: _t0,
      );
      final workspace = Workspace(
        id: 'w-alera-main',
        projectId: project.id,
        name: 'Main',
        branch: 'main',
        path: '/repo/p-alera/w-alera-main',
        createdAt: _t0,
        updatedAt: _t0,
        kind: .main,
        status: .active,
      );
      final first = _tab('t-1', workspace.id);
      final second = _tab('t-2', workspace.id);
      final state = WorkbenchState(
        projects: <Project>[project],
        workspacesByProject: <String, List<Workspace>>{
          project.id: <Workspace>[workspace],
        },
        tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
          workspace.id: <WorkspaceTabRecord>[first, second],
        },
        viewPrefs: .defaults,
        bootstrapped: true,
      );

      final rows = buildSidebarRows(
        state,
        agentStatuses: <String, AgentStatusEntry>{
          first.terminalSessionId: _status(first, .done),
          second.terminalSessionId: _status(second, .working),
        },
      );
      final main = rows.whereType<WorkbenchWorkspaceRow>().single;

      expect(main.agentRuns.map((run) => run.tab.id), <String>['t-1', 't-2']);
      expect(main.aggregateStatus?.state, AgentStatusState.working);
      expect(main.aggregateStatus?.tabId, 't-2');
    },
  );

  group('sidebar agent counts', () {
    late Project project;
    late Workspace alpha;
    late Workspace beta;
    late WorkspaceTabRecord alphaDone;
    late WorkspaceTabRecord alphaWorking;
    late WorkspaceTabRecord betaWaiting;
    late WorkspaceTabRecord betaDone;

    WorkbenchState state({WorkbenchViewPrefs? prefs}) {
      return WorkbenchState(
        projects: <Project>[project],
        workspacesByProject: <String, List<Workspace>>{
          project.id: <Workspace>[alpha, beta],
        },
        tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
          alpha.id: <WorkspaceTabRecord>[alphaDone, alphaWorking],
          beta.id: <WorkspaceTabRecord>[betaWaiting, betaDone],
        },
        viewPrefs: prefs ?? .defaults,
        bootstrapped: true,
      );
    }

    Map<String, AgentStatusEntry> statuses() {
      return <String, AgentStatusEntry>{
        alphaDone.terminalSessionId: _status(alphaDone, .done),
        alphaWorking.terminalSessionId: _status(alphaWorking, .working),
        betaWaiting.terminalSessionId: _status(betaWaiting, .waiting),
        betaDone.terminalSessionId: _status(betaDone, .done),
      };
    }

    /// Acknowledges the current completion epoch of beta's done tab.
    Map<String, DateTime> acknowledgements() {
      return <String, DateTime>{betaDone.terminalSessionId: _t0};
    }

    setUp(() {
      project = Project(
        id: 'p-alera',
        name: 'alera',
        repoPath: '/repo/p-alera',
        createdAt: _t0,
        updatedAt: _t0,
      );
      alpha = Workspace(
        id: 'w-alpha',
        projectId: project.id,
        name: 'Alpha',
        branch: 'main',
        path: '/repo/p-alera/w-alpha',
        createdAt: _t0,
        updatedAt: _t0,
        kind: .main,
        status: .active,
      );
      beta = Workspace(
        id: 'w-beta',
        projectId: project.id,
        name: 'Beta',
        branch: 'feature',
        path: '/repo/p-alera/w-beta',
        createdAt: _t0,
        updatedAt: _t0,
        kind: .linked,
        status: .active,
      );
      alphaDone = _tab('t-a1', alpha.id);
      alphaWorking = _tab('t-a2', alpha.id);
      betaWaiting = _tab('t-b1', beta.id);
      betaDone = _tab('t-b2', beta.id);
    });

    test('project header row sums agent counts across its workspaces', () {
      final rows = buildSidebarRows(
        state(),
        agentStatuses: statuses(),
        acknowledgedCompletions: acknowledgements(),
      );
      final header = rows.whereType<WorkbenchProjectHeaderRow>().single;

      expect(header.agentCounts, <WorkspaceAgentGroupKind, int>{
        WorkspaceAgentGroupKind.waiting: 1,
        WorkspaceAgentGroupKind.doneUnacked: 1,
        WorkspaceAgentGroupKind.working: 1,
        WorkspaceAgentGroupKind.done: 1,
      });
    });

    test('workspace rows bucket unacked done runs separately', () {
      final rows = buildSidebarRows(
        state(),
        agentStatuses: statuses(),
        acknowledgedCompletions: acknowledgements(),
      );
      final workspaceRows = rows.whereType<WorkbenchWorkspaceRow>().toList();
      final alphaRow = workspaceRows.singleWhere(
        (row) => row.workspace.id == alpha.id,
      );
      final betaRow = workspaceRows.singleWhere(
        (row) => row.workspace.id == beta.id,
      );

      expect(
        alphaRow.agentRunGroups.map((group) => group.kind),
        <WorkspaceAgentGroupKind>[
          WorkspaceAgentGroupKind.doneUnacked,
          WorkspaceAgentGroupKind.working,
        ],
      );
      expect(
        betaRow.agentRunGroups.map((group) => group.kind),
        <WorkspaceAgentGroupKind>[
          WorkspaceAgentGroupKind.waiting,
          WorkspaceAgentGroupKind.done,
        ],
      );
    });

    test('a collapsed project still reports its aggregate counts', () {
      final prefs = WorkbenchViewPrefs.defaults.copyWith(
        collapsedProjectIds: <String>{project.id},
      );
      final rows = buildSidebarRows(
        state(prefs: prefs),
        agentStatuses: statuses(),
        acknowledgedCompletions: acknowledgements(),
      );
      final header = rows.whereType<WorkbenchProjectHeaderRow>().single;

      expect(header.collapsed, isTrue);
      expect(header.agentCounts[WorkspaceAgentGroupKind.waiting], 1);
      expect(header.agentCounts[WorkspaceAgentGroupKind.doneUnacked], 1);
    });

    test('a project without agent runs gets an empty count map', () {
      final rows = buildSidebarRows(state());
      final header = rows.whereType<WorkbenchProjectHeaderRow>().single;

      expect(header.agentCounts, isEmpty);
    });
  });
}

WorkspaceTabRecord _tab(String id, String workspaceId) {
  return WorkspaceTabRecord(
    id: id,
    workspaceId: workspaceId,
    kind: .terminal,
    title: id,
    createdAt: _t0,
    updatedAt: _t0,
  );
}

AgentStatusEntry _status(WorkspaceTabRecord tab, AgentStatusState state) {
  return AgentStatusEntry(
    terminalSessionId: tab.terminalSessionId,
    workspaceId: tab.workspaceId,
    tabId: tab.id,
    agentType: .codex,
    state: state,
    prompt: state.name,
    updatedAt: _t0,
    stateStartedAt: _t0,
  );
}
