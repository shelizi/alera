import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/application/workspace_agent_status_projection.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('workspace agent run projection cache', () {
    test('reuses unaffected workspace runs during 1000 unrelated updates', () {
      final cache = WorkspaceAgentRunProjectionCache();
      final tabs = <WorkspaceTabRecord>[_tab('stable-1'), _tab('stable-2')];
      final statuses = <String, AgentStatusEntry>{
        for (final tab in tabs) tab.terminalSessionId: _entry(tab, .working),
      };
      final first = cache.project(
        workspaceId: 'workspace-1',
        tabs: tabs,
        agentStatuses: statuses,
      );
      final other = _tab('other');
      for (var i = 0; i < 1000; i++) {
        final next = cache.project(
          workspaceId: 'workspace-1',
          tabs: tabs,
          agentStatuses: {
            ...statuses,
            other.terminalSessionId: _entry(
              other,
              i.isEven ? .working : .waiting,
            ),
          },
        );
        expect(next, same(first));
      }
    });

    test('preserves unchanged prefix and reflects additions, removals and new tabs', () {
      final cache = WorkspaceAgentRunProjectionCache();
      final a = _tab('a');
      final b = _tab('b');
      final c = _tab('c');
      final statuses = <String, AgentStatusEntry>{
        a.terminalSessionId: _entry(a, .working),
        b.terminalSessionId: _entry(b, .working),
      };
      final first = cache.project(
        workspaceId: 'workspace-1',
        tabs: [a, b, c],
        agentStatuses: statuses,
      );
      statuses[b.terminalSessionId] = _entry(b, .done);
      statuses[c.terminalSessionId] = _entry(c, .waiting);
      final changed = cache.project(
        workspaceId: 'workspace-1',
        tabs: [a, b, c],
        agentStatuses: statuses,
      );
      expect(changed.first, same(first.first));
      expect(changed.map((run) => run.status.state), <AgentStatusState>[
        .working,
        .done,
        .waiting,
      ]);
      statuses.remove(c.terminalSessionId);
      final removed = cache.project(
        workspaceId: 'workspace-1',
        tabs: [a, b, c],
        agentStatuses: statuses,
      );
      expect(removed, hasLength(2));
      expect(removed.last, same(changed[1]));
      final replacement = _tab('a');
      final replaced = cache.project(
        workspaceId: 'workspace-1',
        tabs: [replacement, b],
        agentStatuses: statuses,
      );
      expect(replaced.first.tab, same(replacement));
      expect(replaced.last, same(removed.last));
      cache.retainWorkspaces({});
      final afterPrune = cache.project(
        workspaceId: 'workspace-1',
        tabs: [replacement, b],
        agentStatuses: statuses,
      );
      expect(afterPrune, isNot(same(replaced)));
      expect(
        afterPrune.map((run) => run.status),
        replaced.map((run) => run.status),
      );
    });
  });

  group('workspace agent status projection', () {
    test('prioritizes attention states over working and done', () {
      final doneTab = _tab('tab-done');
      final workingTab = _tab('tab-working');
      final waitingTab = _tab('tab-waiting');
      final blockedTab = _tab('tab-blocked');

      final status = aggregateWorkspaceAgentStatus(
        tabs: <WorkspaceTabRecord>[doneTab, workingTab, waitingTab, blockedTab],
        agentStatuses: <String, AgentStatusEntry>{
          doneTab.terminalSessionId: _entry(doneTab, .done),
          workingTab.terminalSessionId: _entry(workingTab, .working),
          waitingTab.terminalSessionId: _entry(waitingTab, .waiting),
          blockedTab.terminalSessionId: _entry(blockedTab, .blocked),
        },
      );

      expect(status?.tabId, blockedTab.id);
    });

    test('uses most recent update when priorities match', () {
      final firstTab = _tab('tab-1');
      final secondTab = _tab('tab-2');

      final status = aggregateWorkspaceAgentStatus(
        tabs: <WorkspaceTabRecord>[firstTab, secondTab],
        agentStatuses: <String, AgentStatusEntry>{
          firstTab.terminalSessionId: _entry(
            firstTab,
            .waiting,
            updatedAt: .utc(2026, 5, 26, 12),
          ),
          secondTab.terminalSessionId: _entry(
            secondTab,
            .waiting,
            updatedAt: .utc(2026, 5, 26, 12, 1),
          ),
        },
      );

      expect(status?.tabId, secondTab.id);
    });

    test('keeps visible runs in creation order regardless of status churn', () {
      final doneTab = _tab('tab-done');
      final waitingTab = _tab('tab-waiting');
      final workingTab = _tab('tab-working');

      final runs = visibleWorkspaceAgentRuns(
        tabs: <WorkspaceTabRecord>[doneTab, waitingTab, workingTab],
        agentStatuses: <String, AgentStatusEntry>{
          doneTab.terminalSessionId: _entry(doneTab, .done),
          waitingTab.terminalSessionId: _entry(
            waitingTab,
            .waiting,
            updatedAt: .utc(2026, 5, 26, 11),
          ),
          workingTab.terminalSessionId: _entry(
            workingTab,
            .working,
            updatedAt: .utc(2026, 5, 26, 12),
          ),
        },
      );

      expect(runs.map((run) => run.tab.id), <String>[
        'tab-done',
        'tab-waiting',
        'tab-working',
      ]);
    });

    test('ignores non-terminal tabs and stale tab/session ids', () {
      final editorTab = _tab('editor', kind: .editor);
      final terminalTab = _tab('terminal');
      final mismatchedSessionTab = _tab('mismatched-session');

      final status = aggregateWorkspaceAgentStatus(
        tabs: <WorkspaceTabRecord>[
          editorTab,
          terminalTab,
          mismatchedSessionTab,
        ],
        agentStatuses: <String, AgentStatusEntry>{
          editorTab.terminalSessionId: _entry(editorTab, .blocked),
          terminalTab.terminalSessionId: _entry(
            terminalTab,
            .working,
            tabId: 'old-tab',
          ),
          mismatchedSessionTab.terminalSessionId: _entry(
            mismatchedSessionTab,
            .working,
            terminalSessionId: 'old-session',
          ),
        },
      );

      expect(status, isNull);
    });

    test('ranks an interrupted run above later working and done runs', () {
      final interruptedTab = _tab('tab-interrupted');
      final workingTab = _tab('tab-working');
      final doneTab = _tab('tab-done');
      final runs = visibleWorkspaceAgentRuns(
        tabs: <WorkspaceTabRecord>[interruptedTab, workingTab, doneTab],
        agentStatuses: <String, AgentStatusEntry>{
          interruptedTab.terminalSessionId: _entry(
            interruptedTab,
            .done,
            interrupted: true,
          ),
          workingTab.terminalSessionId: _entry(
            workingTab,
            .working,
            updatedAt: .utc(2026, 5, 26, 12),
          ),
          doneTab.terminalSessionId: _entry(
            doneTab,
            .done,
            updatedAt: .utc(2026, 5, 26, 12, 1),
          ),
        },
      );

      expect(mostUrgentWorkspaceAgentRun(runs)?.tab.id, interruptedTab.id);
    });

    test('ranks a later working run above an earlier done run', () {
      final doneTab = _tab('tab-done');
      final workingTab = _tab('tab-working');
      final runs = visibleWorkspaceAgentRuns(
        tabs: <WorkspaceTabRecord>[doneTab, workingTab],
        agentStatuses: <String, AgentStatusEntry>{
          doneTab.terminalSessionId: _entry(doneTab, .done),
          workingTab.terminalSessionId: _entry(workingTab, .working),
        },
      );

      expect(runs.map((run) => run.tab.id), <String>[
        'tab-done',
        'tab-working',
      ]);
      expect(
        mostUrgentWorkspaceAgentRun(runs)?.status.state,
        AgentStatusState.working,
      );
    });

    test('counts waiting, blocked, and interrupted runs as pending review', () {
      final waiting = _tab('tab-waiting');
      final blocked = _tab('tab-blocked');
      final interrupted = _tab('tab-interrupted');
      final working = _tab('tab-working');
      final done = _tab('tab-done');
      final unmatched = _tab('tab-unmatched');

      final runs = visibleWorkspaceAgentRuns(
        tabs: <WorkspaceTabRecord>[
          waiting,
          blocked,
          interrupted,
          working,
          done,
          unmatched,
        ],
        agentStatuses: <String, AgentStatusEntry>{
          waiting.terminalSessionId: _entry(waiting, .waiting),
          blocked.terminalSessionId: _entry(blocked, .blocked),
          interrupted.terminalSessionId: _entry(
            interrupted,
            .done,
            interrupted: true,
          ),
          working.terminalSessionId: _entry(working, .working),
          done.terminalSessionId: _entry(done, .done),
        },
      );

      expect(pendingReviewAgentCount(runs), 3);
    });
  });
}

WorkspaceTabRecord _tab(
  String id, {
  WorkspaceTabKind kind = WorkspaceTabKind.terminal,
}) {
  return WorkspaceTabRecord(
    id: id,
    workspaceId: 'workspace-1',
    title: id,
    kind: kind,
    createdAt: .utc(2026, 5, 26),
    updatedAt: .utc(2026, 5, 26),
  );
}

AgentStatusEntry _entry(
  WorkspaceTabRecord tab,
  AgentStatusState state, {
  DateTime? updatedAt,
  String? tabId,
  String? terminalSessionId,
  bool? interrupted,
}) {
  final timestamp = updatedAt ?? DateTime.utc(2026, 5, 26);
  return AgentStatusEntry(
    terminalSessionId: terminalSessionId ?? tab.terminalSessionId,
    workspaceId: tab.workspaceId,
    tabId: tabId ?? tab.id,
    agentType: .codex,
    state: state,
    prompt: '',
    updatedAt: timestamp,
    stateStartedAt: timestamp,
    interrupted: interrupted,
  );
}
