import 'package:alera/src/features/agent_status/domain/agent_context_overview.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

/// Deterministic dashboard sections. Every Agent appears in exactly one
/// section; a finished/interrupted turn does not appear as Waiting.
enum GlobalAgentDashboardGroup {
  blocked('Blocked'),
  waiting('Waiting'),
  working('Working'),
  interrupted('Interrupted'),
  done('Done');

  GlobalAgentDashboardGroup(this.label);
  final String label;

  static GlobalAgentDashboardGroup forStatus(AgentStatusEntry status) {
    if (status.interrupted == true) return interrupted;
    return switch (status.state) {
      AgentStatusState.blocked => blocked,
      AgentStatusState.waiting => waiting,
      AgentStatusState.working => working,
      AgentStatusState.done => done,
    };
  }
}

class GlobalAgentDashboardSection {
  const GlobalAgentDashboardSection(this.group, this.rows);
  final GlobalAgentDashboardGroup group;
  final List<GlobalAgentLiveRow> rows;
}

/// Group live Agent cards by current Hook state. The Dashboard may opt in to
/// persisted Tab pins first; other consumers keep newest-first ordering.
/// Empty sections are omitted so the viewport stays compact.
List<GlobalAgentDashboardSection> groupGlobalAgentLiveRows(
  Iterable<GlobalAgentLiveRow> rows, {
  bool pinnedFirst = false,
}) {
  final grouped = <GlobalAgentDashboardGroup, List<GlobalAgentLiveRow>>{};
  for (final row in rows) {
    grouped
        .putIfAbsent(
          GlobalAgentDashboardGroup.forStatus(row.status),
          () => <GlobalAgentLiveRow>[],
        )
        .add(row);
  }
  return <GlobalAgentDashboardSection>[
    for (final group in GlobalAgentDashboardGroup.values)
      if (grouped[group] case final sectionRows?)
        GlobalAgentDashboardSection(
          group,
          sectionRows..sort((a, b) {
            if (pinnedFirst && a.tab.isPinned != b.tab.isPinned) {
              return a.tab.isPinned ? -1 : 1;
            }
            final recent = b.status.updatedAt.compareTo(a.status.updatedAt);
            return recent != 0 ? recent : a.tab.id.compareTo(b.tab.id);
          }),
        ),
  ];
}

/// Flatten status sections for a single-card-per-row Dashboard. A collapsed
/// section still renders its label/count and hides only its Agent cards;
/// no Host data or Session ownership is changed.
List<Object> globalAgentDashboardVisibleItems(
  Iterable<GlobalAgentLiveRow> rows,
  Set<GlobalAgentDashboardGroup> collapsed,
) => <Object>[
  for (final section in groupGlobalAgentLiveRows(rows)) ...<Object>[
    section,
    if (!collapsed.contains(section.group)) ...section.rows,
  ],
];

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
    required this.timeline,
  });

  final String project;
  final String workspace;
  final WorkspaceTabRecord tab;
  final AgentStatusEntry status;
  final String goal;
  final String progress;

  /// Host-validated context events; no raw terminal output or tool inputs.
  final List<AgentContextMilestone> timeline;

  /// Last verified Hook state suggests review, not proven user approval.
  bool get needsReview =>
      status.interrupted != true &&
      (status.state == AgentStatusState.waiting ||
          status.state == AgentStatusState.blocked);

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
            timeline: overview.timeline,
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
