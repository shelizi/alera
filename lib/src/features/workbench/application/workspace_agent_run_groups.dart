import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_acknowledgements.dart';
import 'package:alera/src/features/workbench/application/workspace_agent_status_projection.dart';

/// Display buckets for the compact agent summary. Enum order is display
/// order: attention-needing states surface first.
enum WorkspaceAgentGroupKind {
  waiting,
  blocked,
  interrupted,
  doneUnacked,
  working,
  done,
}

class const WorkspaceAgentRunGroup({
  required final WorkspaceAgentGroupKind kind,
  required final List<WorkspaceAgentRun> runs,
});

/// Groups a workspace's agent runs by visual state for the compact summary
/// pill. Interruption wins over the reported state, mirroring the per-row
/// indicator. [acknowledgedCompletions] maps a terminal session id to the
/// completion epoch (`stateStartedAt`) the user has already viewed; done runs
/// past that view land in [WorkspaceAgentGroupKind.doneUnacked].
List<WorkspaceAgentRunGroup> groupWorkspaceAgentRuns(
  List<WorkspaceAgentRun> runs, {
  Map<String, DateTime> acknowledgedCompletions = const <String, DateTime>{},
}) {
  final byKind = <WorkspaceAgentGroupKind, List<WorkspaceAgentRun>>{};
  for (final run in runs) {
    final kind = _groupKindOf(run.status, acknowledgedCompletions);
    byKind.putIfAbsent(kind, () => <WorkspaceAgentRun>[]).add(run);
  }
  return <WorkspaceAgentRunGroup>[
    for (final kind in WorkspaceAgentGroupKind.values)
      if (byKind[kind] case final List<WorkspaceAgentRun> grouped)
        WorkspaceAgentRunGroup(kind: kind, runs: grouped),
  ];
}

WorkspaceAgentGroupKind _groupKindOf(
  AgentStatusEntry status,
  Map<String, DateTime> acknowledgedCompletions,
) {
  if (status.interrupted ?? false) {
    return WorkspaceAgentGroupKind.interrupted;
  }
  return switch (status.state) {
    AgentStatusState.waiting => WorkspaceAgentGroupKind.waiting,
    AgentStatusState.blocked => WorkspaceAgentGroupKind.blocked,
    AgentStatusState.working => WorkspaceAgentGroupKind.working,
    AgentStatusState.done =>
      isCompletionAcknowledged(acknowledgedCompletions, status)
          ? WorkspaceAgentGroupKind.done
          : WorkspaceAgentGroupKind.doneUnacked,
  };
}
