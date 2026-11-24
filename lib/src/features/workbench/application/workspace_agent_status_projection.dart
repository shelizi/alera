import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

class const WorkspaceAgentRun({
  required final WorkspaceTabRecord tab,
  required final AgentStatusEntry status,
});

/// Retains the last visible run list for each workspace. The controller
/// publishes new entries for changed sessions and reuses unaffected entries.
/// Compare actual matching tabs and status identities, so unrelated events
/// do not allocate new run objects or lists for every background workspace.
class WorkspaceAgentRunProjectionCache {
  final _byWorkspace = <String, List<WorkspaceAgentRun>>{};

  List<WorkspaceAgentRun> project({
    required String workspaceId,
    required Iterable<WorkspaceTabRecord> tabs,
    required Map<String, AgentStatusEntry> agentStatuses,
  }) {
    final previous = _byWorkspace[workspaceId] ?? const <WorkspaceAgentRun>[];
    List<WorkspaceAgentRun>? next;
    var index = 0;
    for (final tab in tabs) {
      final status = matchingAgentStatusForTab(
        tab: tab,
        agentStatuses: agentStatuses,
      );
      if (status == null) {
        continue;
      }
      final old = index < previous.length ? previous[index] : null;
      final unchanged =
          old != null &&
          identical(old.tab, tab) &&
          identical(old.status, status);
      if (next == null && !unchanged) {
        next = previous.sublist(0, index);
      }
      if (next != null) {
        next.add(unchanged ? old : WorkspaceAgentRun(tab: tab, status: status));
      }
      index++;
    }
    if (next == null && index == previous.length) {
      return previous;
    }
    final projected = List<WorkspaceAgentRun>.unmodifiable(
      next ?? previous.sublist(0, index),
    );
    if (projected.isEmpty) {
      _byWorkspace.remove(workspaceId);
    } else {
      _byWorkspace[workspaceId] = projected;
    }
    return projected;
  }

  void retainWorkspaces(Map<String, List<WorkspaceTabRecord>> tabsByWorkspace) {
    _byWorkspace.removeWhere(
      (workspaceId, _) => !tabsByWorkspace.containsKey(workspaceId),
    );
  }
}

// Runs keep the incoming tab order (creation order): sorting by status or
// recency would reshuffle sidebar rows on every agent hook event.
List<WorkspaceAgentRun> visibleWorkspaceAgentRuns({
  required Iterable<WorkspaceTabRecord> tabs,
  required Map<String, AgentStatusEntry> agentStatuses,
}) {
  final runs = <WorkspaceAgentRun>[];
  for (final tab in tabs) {
    final entry = matchingAgentStatusForTab(
      tab: tab,
      agentStatuses: agentStatuses,
    );
    if (entry == null) {
      continue;
    }
    runs.add(WorkspaceAgentRun(tab: tab, status: entry));
  }
  return runs;
}

AgentStatusEntry? aggregateWorkspaceAgentStatus({
  required Iterable<WorkspaceTabRecord> tabs,
  required Map<String, AgentStatusEntry> agentStatuses,
}) {
  return mostUrgentWorkspaceAgentRun(
    visibleWorkspaceAgentRuns(tabs: tabs, agentStatuses: agentStatuses),
  )?.status;
}

/// Picks the run that should drive the workspace glyph.
///
/// Visible runs stay in creation order so expanded sidebar rows do not
/// reshuffle; this ranking is only for the single status shown beside the
/// workspace name.
WorkspaceAgentRun? mostUrgentWorkspaceAgentRun(
  Iterable<WorkspaceAgentRun> runs,
) {
  final iterator = runs.iterator;
  if (!iterator.moveNext()) {
    return null;
  }
  var mostUrgent = iterator.current;
  while (iterator.moveNext()) {
    final run = iterator.current;
    if (_compareAgentRuns(run, mostUrgent) < 0) {
      mostUrgent = run;
    }
  }
  return mostUrgent;
}

AgentStatusEntry? matchingAgentStatusForTab({
  required WorkspaceTabRecord tab,
  required Map<String, AgentStatusEntry> agentStatuses,
}) {
  if (tab.kind != WorkspaceTabKind.terminal) {
    return null;
  }
  final entry = agentStatuses[tab.terminalSessionId];
  if (entry == null ||
      entry.workspaceId != tab.workspaceId ||
      entry.tabId != tab.id ||
      entry.terminalSessionId != tab.terminalSessionId) {
    return null;
  }
  return entry;
}

// Urgency ranking for the aggregate workspace status only; row order stays
// in creation order.
int _compareAgentRuns(WorkspaceAgentRun a, WorkspaceAgentRun b) {
  final aPriority = _workspaceStatusPriority(a.status);
  final bPriority = _workspaceStatusPriority(b.status);
  if (aPriority != bPriority) {
    return bPriority.compareTo(aPriority);
  }
  final recency = b.status.updatedAt.compareTo(a.status.updatedAt);
  if (recency != 0) {
    return recency;
  }
  return a.tab.createdAt.compareTo(b.tab.createdAt);
}

/// Agents that currently need the user: waiting, blocked, or interrupted.
int pendingReviewAgentCount(Iterable<WorkspaceAgentRun> runs) {
  var count = 0;
  for (final run in runs) {
    if (agentStatusNeedsYou(run.status)) {
      count += 1;
    }
  }
  return count;
}

/// Shared with Agent Activity: interruption and waiting/blocked need the user.
bool agentStatusNeedsYou(AgentStatusEntry status) {
  if (status.interrupted ?? false) {
    return true;
  }
  return switch (status.state) {
    AgentStatusState.waiting || AgentStatusState.blocked => true,
    AgentStatusState.working || AgentStatusState.done => false,
  };
}

// Compact tray and Agent Activity treat interruption as attention, not as
// a finished run. Ranking it above working/done keeps the glyph from
// turning green (or spinning) over a leftover interrupted tab.
int _workspaceStatusPriority(AgentStatusEntry status) {
  if (status.interrupted ?? false) {
    return 3;
  }
  return switch (status.state) {
    AgentStatusState.blocked => 5,
    AgentStatusState.waiting => 4,
    AgentStatusState.working => 2,
    AgentStatusState.done => 1,
  };
}
