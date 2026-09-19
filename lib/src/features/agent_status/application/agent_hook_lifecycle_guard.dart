import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/agent_status/infra/agent_hook_event_normalizer.dart';

final class AgentHookLifecycleGuard {
  final Map<String, AgentHookLifecyclePolicy> _policies = {};
  final Map<String, String> _workspaceByTerminal = {};

  bool shouldApply(AgentHookEvent event) {
    _workspaceByTerminal[event.terminalSessionId] = event.workspaceId;
    final eventName = agentHookEventName(event);
    if (eventName == null) {
      return true;
    }
    if (!agentTypesWithHttpHooks.contains(event.agentType)) {
      return false;
    }
    final policy = _policies.putIfAbsent(
      event.agentType.key,
      () => createAgentHookLifecyclePolicy(event.agentType),
    );
    return policy.shouldApply(event, eventName);
  }

  void clearTerminal(String terminalSessionId) {
    for (final policy in _policies.values) {
      policy.clearTerminal(terminalSessionId);
    }
    _workspaceByTerminal.remove(terminalSessionId);
  }

  void clearWorkspace(String workspaceId) {
    final terminalSessionIds = <String>[
      for (final entry in _workspaceByTerminal.entries)
        if (entry.value == workspaceId) entry.key,
    ];
    for (final terminalSessionId in terminalSessionIds) {
      clearTerminal(terminalSessionId);
    }
  }

  void reset() {
    for (final policy in _policies.values) {
      policy.reset();
    }
    _policies.clear();
    _workspaceByTerminal.clear();
  }
}
