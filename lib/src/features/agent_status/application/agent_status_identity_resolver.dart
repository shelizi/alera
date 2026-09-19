import 'package:alera/src/features/agent_status/domain/agent_status.dart';

const agentStatusIdentityStaleThreshold = Duration(minutes: 30);

class const AgentStatusIdentityResolution({
  required final AgentType effectiveAgentType,
  required final bool inheritedFromActiveTerminal,
  required final bool shouldIgnoreEvent,
});

AgentStatusIdentityResolution resolveAgentStatusIdentity({
  required AgentStatusEntry? previous,
  required AgentType incomingAgentType,
  required AgentStatusState normalizedState,
  required DateTime receivedAt,
  required Duration staleThreshold,
  required bool shouldTakeOverActiveTerminal,
}) {
  final inheritedFromActiveTerminal =
      previous != null &&
      previous.state != AgentStatusState.done &&
      previous.agentType != incomingAgentType &&
      !_isStale(previous, receivedAt, staleThreshold) &&
      !shouldTakeOverActiveTerminal;
  final effectiveAgentType = inheritedFromActiveTerminal
      ? previous.agentType
      : incomingAgentType;
  return AgentStatusIdentityResolution(
    effectiveAgentType: effectiveAgentType,
    inheritedFromActiveTerminal: inheritedFromActiveTerminal,
    shouldIgnoreEvent:
        inheritedFromActiveTerminal && normalizedState == AgentStatusState.done,
  );
}

bool _isStale(
  AgentStatusEntry entry,
  DateTime receivedAt,
  Duration staleThreshold,
) {
  return receivedAt.difference(entry.updatedAt) > staleThreshold;
}
