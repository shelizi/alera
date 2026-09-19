part of '../agent_hook_event_normalizer.dart';

const _claudeAgentHookAdapter = _ClaudeAgentHookAdapter();

final class _ClaudeAgentHookAdapter extends _AgentHookAdapter {
  const _ClaudeAgentHookAdapter();

  @override
  AgentStatusState? normalizeState(
    AgentHookEvent event,
    String eventName,
    String? toolName,
    AgentStatusEntry? previous,
  ) => _normalizeClaudeState(eventName, toolName);

  @override
  bool isNewTurn(String eventName) => _isClaudeNewTurn(eventName);

  @override
  bool isInterrupted(AgentHookEvent event, String eventName) {
    return _isGenericInterrupted(event, inferFailure: false);
  }

  @override
  bool shouldTakeOverActiveStatusFrom(AgentType previousAgentType) => false;
}

AgentStatusState? _normalizeClaudeState(String eventName, String? toolName) {
  if (eventName == 'AskUserQuestion' ||
      (eventName == 'PreToolUse' && _isHumanInputTool(toolName))) {
    return AgentStatusState.waiting;
  }
  return switch (eventName) {
    'UserPromptSubmit' ||
    'PreToolUse' ||
    'PostToolUse' ||
    'PostToolUseFailure' => AgentStatusState.working,
    'PermissionRequest' => AgentStatusState.waiting,
    'Stop' || 'StopFailure' => AgentStatusState.done,
    _ => null,
  };
}

bool _isClaudeNewTurn(String eventName) {
  return eventName == 'UserPromptSubmit';
}
