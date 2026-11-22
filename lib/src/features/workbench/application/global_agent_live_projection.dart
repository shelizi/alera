import 'package:alera/src/features/agent_status/domain/agent_context_overview.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

/// Live, verified workspace-tab projection. Historical provider transcripts
/// are intentionally excluded until the shared Host search API is available.
class GlobalAgentLiveRow {
  const GlobalAgentLiveRow({
    required this.project,
    required this.workspace,
    required this.tab,
    required this.status,
    required this.goal,
    required this.progress,
  });

  final String project;
  final String workspace;
  final WorkspaceTabRecord tab;
  final AgentStatusEntry status;
  final String goal;
  final String progress;

  bool matches(String query) {
    final q = query.trim().toLowerCase();
    if (q.isEmpty) return true;
    final terms = q.split(RegExp(r'\s+'));
    final text =
        '$project $workspace ${tab.title} '
                '${status.agentType.key} $goal $progress'
            .toLowerCase();
    return terms.every(text.contains);
  }
}

List<GlobalAgentLiveRow> projectGlobalAgentLiveRows(
  WorkbenchState state,
  Map<String, AgentStatusEntry> statuses,
) {
  final rows = <GlobalAgentLiveRow>[];
  for (final project in state.projects) {
    for (final workspace in state.workspacesFor(project.id)) {
      if (!workspace.isActive) continue;
      for (final tab in state.tabsFor(workspace.id)) {
        if (tab.kind != WorkspaceTabKind.terminal) continue;
        final status = statuses[tab.terminalSessionId];
        if (status == null ||
            status.workspaceId != workspace.id ||
            status.tabId != tab.id ||
            status.terminalSessionId != tab.terminalSessionId) {
          continue;
        }
        final overview = AgentContextOverview.fromRaw(status.contextOverview);
        rows.add(
          GlobalAgentLiveRow(
            project: project.name,
            workspace: workspace.name,
            tab: tab,
            status: status,
            goal: overview.goal.isNotEmpty ? overview.goal : status.prompt,
            progress: overview.progress.isNotEmpty
                ? overview.progress
                : (status.lastAssistantMessage ?? ''),
          ),
        );
      }
    }
  }
  rows.sort((a, b) => b.status.updatedAt.compareTo(a.status.updatedAt));
  return rows;
}

/// Preflight for Dashboard commands. The Host still validates the identity
/// atomically immediately before PTY input, preventing transfer races.
bool globalAgentTargetIsCurrent({
  required WorkbenchState workbench,
  required Map<String, AgentStatusEntry> statuses,
  required String sessionId,
  required String workspaceId,
  required String tabId,
}) {
  final entry = statuses[sessionId];
  if (entry == null ||
      entry.workspaceId != workspaceId ||
      entry.tabId != tabId ||
      entry.terminalSessionId != sessionId) {
    return false;
  }
  final registered = workbench.workspacesByProject.values.any(
    (workspaces) => workspaces.any(
      (workspace) => workspace.id == workspaceId && workspace.isActive,
    ),
  );
  return registered &&
      workbench
          .tabsFor(workspaceId)
          .any(
            (tab) =>
                tab.kind == WorkspaceTabKind.terminal &&
                tab.id == tabId &&
                tab.terminalSessionId == sessionId,
          );
}
