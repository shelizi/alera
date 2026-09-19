part of '../agent_hook_event_normalizer.dart';

const _codexAgentHookAdapter = _CodexAgentHookAdapter();

final class _CodexAgentHookAdapter extends _AgentHookAdapter {
  const _CodexAgentHookAdapter();

  @override
  AgentStatusState? normalizeState(
    AgentHookEvent event,
    String eventName,
    String? toolName,
    AgentStatusEntry? previous,
  ) => _normalizeCodexState(eventName, toolName);

  @override
  bool isNewTurn(String eventName) => _isCodexNewTurn(eventName);

  @override
  _NestedToolCall nestedToolCall(Map<String, Object?> payload) {
    return _readCodexToolCall(payload);
  }
}

AgentStatusState? _normalizeCodexState(String eventName, String? toolName) {
  if (eventName == 'PreToolUse' && _isHumanInputTool(toolName)) {
    return AgentStatusState.waiting;
  }
  return switch (eventName) {
    'SessionStart' ||
    'UserPromptSubmit' ||
    'PreToolUse' ||
    'PostToolUse' => AgentStatusState.working,
    'PermissionRequest' => AgentStatusState.waiting,
    'Stop' || 'Interrupt' || 'SessionEnd' => AgentStatusState.done,
    _ => null,
  };
}

bool _isCodexNewTurn(String eventName) {
  return eventName == 'SessionStart' || eventName == 'UserPromptSubmit';
}

_NestedToolCall _readCodexToolCall(Map<String, Object?> payload) {
  final toolCall = payload['toolCall'] ?? payload['tool_call'];
  if (toolCall is Map) {
    final record = Map<String, Object?>.from(toolCall);
    return _NestedToolCall(
      toolName: _readFirstString(record, const <String>[
        'name',
        'toolName',
        'tool_name',
      ]),
      toolInputSource:
          _parseJsonObjectString(record['args']) ??
          record['args'] ??
          _parseJsonObjectString(record['arguments']) ??
          record['arguments'],
    );
  }

  final toolCalls = payload['toolCalls'] ?? payload['tool_calls'];
  if (toolCalls is List && toolCalls.isNotEmpty && toolCalls.first is Map) {
    final record = Map<String, Object?>.from(toolCalls.first as Map);
    return _NestedToolCall(
      toolName: _readFirstString(record, const <String>[
        'name',
        'toolName',
        'tool_name',
      ]),
      toolInputSource:
          _parseJsonObjectString(record['args']) ??
          record['args'] ??
          _parseJsonObjectString(record['arguments']) ??
          record['arguments'],
    );
  }

  return const _NestedToolCall();
}
