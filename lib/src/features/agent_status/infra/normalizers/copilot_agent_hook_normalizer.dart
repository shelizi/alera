part of '../agent_hook_event_normalizer.dart';

const _copilotAgentHookAdapter = _CopilotAgentHookAdapter();

final class _CopilotAgentHookAdapter extends _AgentHookAdapter {
  const _CopilotAgentHookAdapter();

  @override
  String? normalizeEventName(AgentHookEvent event, String? rawEventName) {
    return _normalizeCopilotEventName(
      rawEventName ?? _inferCopilotEventName(event.payload),
    );
  }

  @override
  AgentStatusState? normalizeState(
    AgentHookEvent event,
    String eventName,
    String? toolName,
    AgentStatusEntry? previous,
  ) => _normalizeCopilotState(eventName, event.payload, toolName);

  @override
  bool isNewTurn(String eventName) => _isCopilotNewTurn(eventName);

  @override
  bool isSessionClose(String eventName) => eventName == 'SessionEnd';

  @override
  String promptForEvent(AgentHookEvent event, String eventName) {
    return eventName == 'Notification'
        ? ''
        : super.promptForEvent(event, eventName);
  }

  @override
  _NestedToolCall nestedToolCall(Map<String, Object?> payload) {
    return _readCopilotToolCall(payload);
  }
}

AgentStatusState? _normalizeCopilotState(
  String eventName,
  Map<String, Object?> payload,
  String? toolName,
) {
  final notificationType = _readFirstString(payload, const <String>[
    'notification_type',
    'notificationType',
  ]);
  final isBlockingNotification =
      eventName == 'Notification' &&
      (notificationType == 'permission_prompt' ||
          notificationType == 'elicitation_dialog');
  final isAskUserTool =
      (eventName == 'PreToolUse' || eventName == 'PermissionRequest') &&
      _isHumanInputTool(toolName);
  if (isBlockingNotification || isAskUserTool) {
    return AgentStatusState.blocked;
  }
  return switch (eventName) {
    'SessionStart' ||
    'UserPromptSubmit' ||
    'PreToolUse' ||
    'PostToolUse' ||
    'PostToolUseFailure' ||
    'PermissionRequest' => AgentStatusState.working,
    'Stop' || 'SessionEnd' => AgentStatusState.done,
    'ErrorOccurred' =>
      payload['recoverable'] == true
          ? AgentStatusState.working
          : AgentStatusState.done,
    _ => null,
  };
}

bool _isCopilotNewTurn(String eventName) {
  return eventName == 'SessionStart' || eventName == 'UserPromptSubmit';
}

String? _normalizeCopilotEventName(String? eventName) {
  if (eventName == null) {
    return null;
  }
  return const <String, String>{
        'sessionStart': 'SessionStart',
        'sessionEnd': 'SessionEnd',
        'userPromptSubmitted': 'UserPromptSubmit',
        'userPromptSubmit': 'UserPromptSubmit',
        'preToolUse': 'PreToolUse',
        'postToolUse': 'PostToolUse',
        'postToolUseFailure': 'PostToolUseFailure',
        'subagentStart': 'SubagentStart',
        'subagentStop': 'SubagentStop',
        'preCompact': 'PreCompact',
        'agentStop': 'Stop',
        'stop': 'Stop',
        'errorOccurred': 'ErrorOccurred',
        'permissionRequest': 'PermissionRequest',
        'notification': 'Notification',
      }[eventName] ??
      eventName;
}

String? _inferCopilotEventName(Map<String, Object?> payload) {
  if (_readFirstString(payload, const <String>[
        'initial_prompt',
        'initialPrompt',
      ]) !=
      null) {
    return 'SessionStart';
  }
  if (_readFirstString(payload, const <String>['prompt']) != null) {
    return 'UserPromptSubmit';
  }
  if (_readFirstString(payload, const <String>[
        'notification_type',
        'notificationType',
      ]) !=
      null) {
    return 'Notification';
  }
  if (_readFirstString(payload, const <String>[
        'transcript_path',
        'transcriptPath',
        'stop_reason',
        'stopReason',
      ]) !=
      null) {
    return 'Stop';
  }
  if (payload['error'] != null ||
      _readFirstString(payload, const <String>[
            'error_context',
            'errorContext',
          ]) !=
          null) {
    return 'ErrorOccurred';
  }
  if (payload['toolCalls'] is List ||
      _readFirstString(payload, const <String>[
            'tool_name',
            'toolName',
            'name',
          ]) !=
          null) {
    if (payload['tool_result'] != null ||
        payload['toolResult'] != null ||
        payload['tool_response'] != null ||
        payload['toolResponse'] != null) {
      return 'PostToolUse';
    }
    return 'PreToolUse';
  }
  return null;
}

_NestedToolCall _readCopilotToolCall(Map<String, Object?> payload) {
  final toolCalls = payload['toolCalls'];
  if (toolCalls is! List || toolCalls.isEmpty || toolCalls.first is! Map) {
    return const _NestedToolCall();
  }
  final record = Map<String, Object?>.from(toolCalls.first as Map);
  final args =
      _parseJsonObjectString(record['args']) ??
      record['args'] ??
      _parseJsonObjectString(record['arguments']) ??
      record['arguments'];
  return _NestedToolCall(
    toolName: _readFirstString(record, const <String>[
      'name',
      'toolName',
      'tool_name',
    ]),
    toolInputSource: args,
  );
}
