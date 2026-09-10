import 'package:alera/src/features/agent_status/domain/agent_status.dart';

/// Session-local acknowledgement of the exact completion epoch a user viewed.
final class WorkbenchTabCompletionAcknowledgements {
  final Map<String, DateTime> _stateStartedAtByTerminal = <String, DateTime>{};

  void acknowledge(AgentStatusEntry? status) {
    if (status?.state != AgentStatusState.done) {
      return;
    }
    _stateStartedAtByTerminal[status!.terminalSessionId] =
        status.stateStartedAt;
  }

  bool isAcknowledged(AgentStatusEntry? status) {
    if (status?.state != AgentStatusState.done) {
      return false;
    }
    return _stateStartedAtByTerminal[status!.terminalSessionId] ==
        status.stateStartedAt;
  }

  void retainTerminalSessions(Set<String> terminalSessionIds) {
    _stateStartedAtByTerminal.removeWhere(
      (sessionId, _) => !terminalSessionIds.contains(sessionId),
    );
  }
}

/// Attention affordance for a workspace tab chip beyond the raw agent state.
enum WorkbenchTabAttention {
  none,
  agentWaiting,
  agentBlocked,
  agentDoneUnacked,
}

/// Derive tab attention: waiting/blocked always need attention; a completion
/// needs attention until its specific state transition has been acknowledged.
WorkbenchTabAttention workbenchTabAttention({
  required AgentStatusEntry? status,
  required bool completionAcknowledged,
}) {
  final entry = status;
  if (entry == null) {
    return WorkbenchTabAttention.none;
  }
  return switch (entry.state) {
    AgentStatusState.waiting => WorkbenchTabAttention.agentWaiting,
    AgentStatusState.blocked => WorkbenchTabAttention.agentBlocked,
    AgentStatusState.done when !completionAcknowledged =>
      WorkbenchTabAttention.agentDoneUnacked,
    AgentStatusState.working ||
    AgentStatusState.done => WorkbenchTabAttention.none,
  };
}
