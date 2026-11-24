import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_listing.dart';
import 'package:alera/src/features/workbench/application/workbench_sidebar_view.dart';
import 'package:alera/src/features/workbench/application/workspace_agent_run_groups.dart';
import 'package:alera/src/features/workbench/application/workspace_agent_status_projection.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

final _now = DateTime.utc(2026, 11, 24);
final _project = Project(
  id: 'project',
  name: 'Project',
  repoPath: '/repo',
  createdAt: _now,
  updatedAt: _now,
);
Workspace _workspace(String id, {bool archived = false}) => Workspace(
  id: id,
  projectId: _project.id,
  name: id,
  path: '/repo/$id',
  createdAt: _now,
  updatedAt: _now,
  kind: .linked,
  status: .active,
  archivedAt: archived ? _now : null,
);

WorkspaceAgentRun _run(Workspace workspace, String id, {String message = ''}) {
  final tab = WorkspaceTabRecord(
    id: id,
    workspaceId: workspace.id,
    title: id,
    kind: .terminal,
    createdAt: _now,
    updatedAt: _now,
  );
  return WorkspaceAgentRun(
    tab: tab,
    status: AgentStatusEntry(
      terminalSessionId: tab.terminalSessionId,
      workspaceId: workspace.id,
      tabId: tab.id,
      agentType: .codex,
      state: .done,
      prompt: '',
      lastAssistantMessage: message,
      updatedAt: _now,
      stateStartedAt: _now,
    ),
  );
}

WorkbenchWorkspaceRow _row(
  Workspace workspace, {
  List<WorkspaceAgentRun> runs = const [],
  Map<String, DateTime> acknowledged = const {},
  bool pinned = false,
}) => WorkbenchWorkspaceRow(
  project: _project,
  workspace: workspace,
  showProjectChip: false,
  indent: 1,
  expanded: true,
  agentRuns: runs,
  agentRunGroups: groupWorkspaceAgentRuns(
    runs,
    acknowledgedCompletions: acknowledged,
  ),
  isPinnedCopy: pinned,
);

WorkbenchProjectHeaderRow _header(int count) => WorkbenchProjectHeaderRow(
  project: _project,
  workspaceCount: 2,
  collapsed: false,
  agentCounts: {WorkspaceAgentGroupKind.doneUnacked: count},
);

void main() {
  test(
    '1,000 status updates preserve layout and unaffected row identities',
    () {
      final cache = WorkbenchSidebarViewCache();
      final a = _workspace('a');
      final b = _workspace('b');
      final stableRuns = [_run(b, 'b-tab')];
      WorkbenchSidebarView project(int i) => cache.project(
        [
          _header(2),
          _row(a, runs: [_run(a, 'a-tab', message: '$i')]),
          _row(b, runs: stableRuns),
        ],
        expandedArchivedKeys: const {},
        searchQuery: '',
      );
      final first = project(0);
      var previous = first;
      for (var i = 1; i <= 1000; i++) {
        final next = project(i);
        expect(next.layout, same(first.layout));
        expect(
          next.rowsByKey['project:project'],
          same(first.rowsByKey['project:project']),
        );
        expect(
          next.rowsByKey['workspace:all:b'],
          same(first.rowsByKey['workspace:all:b']),
        );
        expect(
          next.rowsByKey['workspace:all:a'],
          isNot(same(previous.rowsByKey['workspace:all:a'])),
        );
        previous = next;
      }
      expect(
        (previous.rowsByKey['workspace:all:a'] as WorkbenchWorkspaceRow)
            .agentRuns
            .single
            .status
            .lastAssistantMessage,
        '1000',
      );
    },
  );

  test('acknowledgement membership, model replacement and pinned copies remain correct', () {
    final cache = WorkbenchSidebarViewCache();
    final a = _workspace('a');
    final runs = [_run(a, 'one'), _run(a, 'two')];
    WorkbenchSidebarView project(Workspace workspace, int index) =>
        cache.project(
          [
            _row(
              workspace,
              runs: runs,
              acknowledged: {runs[index].status.terminalSessionId: _now},
            ),
            _row(workspace, runs: runs, pinned: true),
          ],
          expandedArchivedKeys: const {},
          searchQuery: '',
        );
    final first = project(a, 0);
    final second = project(a, 1);
    expect(second.layout, same(first.layout));
    expect(
      second.rowsByKey['workspace:pinned:a'],
      same(first.rowsByKey['workspace:pinned:a']),
    );
    final regular =
        second.rowsByKey['workspace:all:a']! as WorkbenchWorkspaceRow;
    expect(regular, isNot(same(first.rowsByKey[regular.key])));
    expect(regular.agentRunGroups.last.runs.single.tab.id, 'two');
    final replacement = a.copyWith(name: 'Renamed');
    final third = project(replacement, 1);
    expect(
      (third.rowsByKey[regular.key] as WorkbenchWorkspaceRow).workspace,
      same(replacement),
    );
  });

  test(
    'archive collapse, search and reorder update only layout when appropriate',
    () {
      final cache = WorkbenchSidebarViewCache();
      final a = _row(_workspace('a'));
      final b = _row(_workspace('b'));
      final archived = _row(_workspace('archived', archived: true));
      final header = WorkbenchArchivedHeaderRow(
        scopeId: 'global',
        workspaceCount: 1,
        indent: 0,
      );
      WorkbenchSidebarView project(
        List<WorkbenchSidebarRow> rows, {
        Set<String> expanded = const {},
        String query = '',
      }) => cache.project(
        rows,
        expandedArchivedKeys: expanded,
        searchQuery: query,
      );
      final first = project([a, b, header, archived]);
      expect(first.layout.keys, [a.key, b.key, header.key]);
      expect(first.rowsByKey.containsKey(archived.key), isFalse);
      final changedHidden = _row(
        archived.workspace,
        runs: [_run(archived.workspace, 'hidden')],
      );
      final hidden = project([a, b, header, changedHidden]);
      expect(hidden, same(first));
      final reordered = project([b, a, header, changedHidden]);
      expect(reordered.layout.keys, [b.key, a.key, header.key]);
      expect(reordered.layout.indexByKey[a.key], 1);
      expect(reordered.rowsByKey[a.key], same(first.rowsByKey[a.key]));
      expect(reordered.layout.previousIsWorkspace(2), isTrue);
      final expanded = project(
        [b, a, header, changedHidden],
        expanded: {header.key},
      );
      expect(expanded.rowsByKey[archived.key], same(changedHidden));
      final searched = project([
        b,
        a,
        header,
        changedHidden,
      ], query: 'archived');
      expect(searched.layout.keys, expanded.layout.keys);
      expect(searched.layout.collapsedArchivedKeys, isEmpty);
      final collapsed = project([b, a, header, changedHidden]);
      expect(collapsed.rowsByKey.containsKey(archived.key), isFalse);
    },
  );
}
