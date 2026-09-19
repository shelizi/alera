part of '../agent_hook_event_normalizer.dart';

_ToolSnapshot _extractToolSnapshot(
  AgentHookEvent event, {
  required String eventName,
  required _AgentHookAdapter adapter,
}) {
  final customSnapshot = adapter.extractToolSnapshot(event, eventName);
  if (customSnapshot != null) {
    return customSnapshot;
  }
  final payload = event.payload;
  final hasToolEvent = adapter.isToolEvent(eventName);
  String? toolName;
  String? toolInput;
  var hasToolInput = false;
  if (hasToolEvent) {
    final nestedToolCall = adapter.nestedToolCall(payload);
    toolName =
        _readFirstString(payload, const <String>[
          'tool_name',
          'toolName',
          'name',
          'tool',
        ]) ??
        nestedToolCall.toolName;
    for (final key in const <String>[
      'tool_input',
      'toolInput',
      'toolArgs',
      'input',
      'arguments',
    ]) {
      if (payload.containsKey(key)) {
        hasToolInput = true;
        toolInput = _deriveToolInputPreview(toolName, payload[key]);
        if (toolInput != null) {
          break;
        }
      }
    }
    if (toolInput == null && nestedToolCall.toolInputSource != null) {
      hasToolInput = true;
      toolInput = _deriveToolInputPreview(
        toolName,
        nestedToolCall.toolInputSource,
      );
    }
  }

  final lastAssistantMessage =
      adapter.assistantText(event, eventName) ??
      _readFirstString(payload, const <String>[
        'last_assistant_message',
        'lastAssistantMessage',
        'message',
      ]) ??
      (eventName == 'Notification'
          ? _readFirstString(payload, const <String>['body', 'text', 'title'])
          : null) ??
      _extractToolResponseText(payload['tool_response']) ??
      _extractToolResponseText(payload['toolResponse']) ??
      _extractToolResponseText(payload['tool_result']) ??
      _extractToolResponseText(payload['toolResult']) ??
      _readFirstString(payload, const <String>[
        'error_message',
        'errorMessage',
        'error',
      ]) ??
      (adapter.shouldReadAssistantTranscript(eventName)
          ? _readLastAssistantFromTranscript(
              payload['transcript_path'] ?? payload['transcriptPath'],
            )
          : null);

  return _ToolSnapshot(
    toolName: toolName,
    toolInput: toolInput,
    lastAssistantMessage: lastAssistantMessage,
    hasToolUpdate: hasToolEvent,
    hasToolInput: hasToolInput,
  );
}
