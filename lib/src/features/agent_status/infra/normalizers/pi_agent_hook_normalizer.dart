part of '../agent_hook_event_normalizer.dart';

const _piAgentHookAdapter = _PiAgentHookAdapter();

final class _PiAgentHookAdapter extends _AgentHookAdapter {
  const _PiAgentHookAdapter();

  @override
  AgentStatusState? normalizeState(
    AgentHookEvent event,
    String eventName,
    String? toolName,
    AgentStatusEntry? previous,
  ) => _normalizePiState(eventName);

  @override
  bool isNewTurn(String eventName) => _isPiNewTurn(eventName);

  @override
  bool isSessionClose(String eventName) => eventName == 'session_shutdown';

  @override
  String? assistantText(AgentHookEvent event, String eventName) {
    return _piAssistantTextForEvent(event, eventName);
  }
}

AgentStatusState? _normalizePiState(String eventName) {
  return switch (eventName) {
    'before_agent_start' ||
    'agent_start' ||
    'tool_call' ||
    'tool_execution_start' ||
    'tool_execution_end' ||
    'message_end' ||
    'agent_end' => AgentStatusState.working,
    'agent_settled' || 'session_shutdown' => AgentStatusState.done,
    _ => null,
  };
}

bool _isPiNewTurn(String eventName) {
  return eventName == 'before_agent_start';
}

String? _piAssistantTextForEvent(AgentHookEvent event, String eventName) {
  if (eventName != 'message_end' || event.payload['role'] != 'assistant') {
    return null;
  }
  final value = event.payload['text'];
  if (value is String && value.trim().isNotEmpty) {
    return _normalizeMultiline(value, 8000);
  }
  return null;
}
