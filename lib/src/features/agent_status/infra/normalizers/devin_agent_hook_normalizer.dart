part of '../agent_hook_event_normalizer.dart';

const _devinAgentHookAdapter = _DevinAgentHookAdapter();

final class _DevinAgentHookAdapter extends _AgentHookAdapter {
  const _DevinAgentHookAdapter();

  @override
  AgentStatusState? normalizeState(
    AgentHookEvent event,
    String eventName,
    String? toolName,
    AgentStatusEntry? previous,
  ) => _normalizeDevinState(eventName);

  @override
  bool isNewTurn(String eventName) {
    return eventName == 'SessionStart' || eventName == 'UserPromptSubmit';
  }

  @override
  bool isSessionClose(String eventName) => eventName == 'SessionEnd';
}

AgentStatusState? _normalizeDevinState(String eventName) {
  return switch (eventName) {
    'SessionStart' ||
    'UserPromptSubmit' ||
    'PreToolUse' ||
    'PostToolUse' => AgentStatusState.working,
    'PermissionRequest' => AgentStatusState.blocked,
    'Stop' || 'SessionEnd' => AgentStatusState.done,
    _ => null,
  };
}
