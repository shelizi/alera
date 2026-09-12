import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_archive_sweep.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

final DateTime _now = .utc(2026, 9, 1, 12);
const Duration _threshold = Duration(days: 30);

Project _project(String id) {
  return Project(
    id: id,
    name: id,
    repoPath: '/repo/$id',
    createdAt: _now,
    updatedAt: _now,
  );
}

Workspace _workspace(
  String id, {
  WorkspaceKind kind = .linked,
  bool pinned = false,
  bool archived = false,
  DateTime? createdAt,
  DateTime? updatedAt,
}) {
  return Workspace(
    id: id,
    projectId: 'project',
    name: id,
    path: '/repo/project/$id',
    createdAt: createdAt ?? _now.subtract(const Duration(days: 60)),
    updatedAt: updatedAt ?? _now.subtract(const Duration(days: 40)),
    kind: kind,
    status: .active,
    isPinned: pinned,
    archivedAt: archived ? _now.subtract(const Duration(days: 5)) : null,
  );
}

AgentStatusEntry _agentEntry(String workspaceId, AgentStatusState state) {
  return AgentStatusEntry(
    terminalSessionId: 'session-$workspaceId',
    workspaceId: workspaceId,
    tabId: 'tab-$workspaceId',
    agentType: .codex,
    state: state,
    prompt: 'prompt',
    updatedAt: _now,
    stateStartedAt: _now,
  );
}

bool _eligible(
  Workspace workspace, {
  WorkbenchState? state,
  Map<String, DateTime> activity = const <String, DateTime>{},
  Map<String, AgentStatusEntry> statuses = const <String, AgentStatusEntry>{},
  Duration threshold = _threshold,
}) {
  return workspaceEligibleForAutoArchive(
    workspace: workspace,
    state: state ?? WorkbenchState(projects: <Project>[_project('project')]),
    lastActivityByWorkspaceId: activity,
    agentStatuses: statuses,
    threshold: threshold,
    now: _now,
  );
}

void main() {
  group('workspaceEligibleForAutoArchive', () {
    test('accepts an idle linked workspace past the threshold', () {
      expect(_eligible(_workspace('idle')), isTrue);
    });

    test('rejects main, pinned and already archived workspaces', () {
      expect(_eligible(_workspace('main', kind: .main)), isFalse);
      expect(_eligible(_workspace('pinned', pinned: true)), isFalse);
      expect(_eligible(_workspace('archived', archived: true)), isFalse);
    });

    test('rejects the active workspace and workspaces with open tabs', () {
      final workspace = _workspace('busy');
      final active = WorkbenchState(
        projects: <Project>[_project('project')],
        workspacesByProject: <String, List<Workspace>>{
          'project': <Workspace>[workspace],
        },
        activeWorkspaceId: workspace.id,
      );
      expect(_eligible(workspace, state: active), isFalse);

      final withTabs = WorkbenchState(
        projects: <Project>[_project('project')],
        workspacesByProject: <String, List<Workspace>>{
          'project': <Workspace>[workspace],
        },
        tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
          workspace.id: <WorkspaceTabRecord>[
            WorkspaceTabRecord(
              id: 'tab-1',
              workspaceId: workspace.id,
              title: 'Terminal',
              createdAt: _now,
              updatedAt: _now,
            ),
          ],
        },
      );
      expect(_eligible(workspace, state: withTabs), isFalse);
    });

    test('rejects workspaces with an unfinished agent run', () {
      final workspace = _workspace('running');
      for (final state in <AgentStatusState>[.working, .waiting, .blocked]) {
        expect(
          _eligible(
            workspace,
            statuses: <String, AgentStatusEntry>{
              'session-running': _agentEntry(workspace.id, state),
            },
          ),
          isFalse,
          reason: 'state $state should keep the workspace listed',
        );
      }
      expect(
        _eligible(
          workspace,
          statuses: <String, AgentStatusEntry>{
            'session-running': _agentEntry(workspace.id, .done),
          },
        ),
        isTrue,
      );
      // A run attached to a different workspace does not block the sweep.
      expect(
        _eligible(
          workspace,
          statuses: <String, AgentStatusEntry>{
            'session-other': _agentEntry('other', .working),
          },
        ),
        isTrue,
      );
    });

    test('uses the newest of activity, updatedAt and createdAt', () {
      final workspace = _workspace('idle');

      // Recent recorded activity keeps an otherwise stale workspace listed.
      expect(
        _eligible(workspace, activity: <String, DateTime>{'idle': _now}),
        isFalse,
      );

      // A recent createdAt wins over an older updatedAt.
      expect(
        _eligible(
          _workspace(
            'fresh',
            createdAt: _now.subtract(const Duration(days: 2)),
            updatedAt: _now.subtract(const Duration(days: 50)),
          ),
        ),
        isFalse,
      );

      // Stale recorded activity does not postpone the sweep.
      expect(
        _eligible(
          workspace,
          activity: <String, DateTime>{
            'idle': _now.subtract(const Duration(days: 90)),
          },
        ),
        isTrue,
      );
    });

    test('archives exactly at the threshold boundary', () {
      final workspace = _workspace(
        'edge',
        updatedAt: _now.subtract(_threshold),
        createdAt: _now.subtract(const Duration(days: 90)),
      );
      expect(_eligible(workspace), isTrue);
      expect(
        _eligible(
          workspace,
          activity: <String, DateTime>{
            'edge': _now.subtract(_threshold).add(const Duration(seconds: 1)),
          },
        ),
        isFalse,
      );
    });
  });

  group('workbenchArchivedSectionsCollapse', () {
    test('toggle adds then removes the header key', () {
      final container = ProviderContainer();
      addTearDown(container.dispose);
      final provider = workbenchArchivedSectionsCollapseProvider;

      expect(container.read(provider), isEmpty);
      container.read(provider.notifier).toggle('archived:global');
      expect(container.read(provider), <String>{'archived:global'});
      container.read(provider.notifier).toggle('archived:global');
      expect(container.read(provider), isEmpty);
    });
  });
}
