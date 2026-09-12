import 'package:alera/src/features/agent_status/domain/agent_status.dart';

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
