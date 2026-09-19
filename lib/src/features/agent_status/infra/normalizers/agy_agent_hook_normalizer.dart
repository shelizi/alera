part of '../agent_hook_event_normalizer.dart';

const _agyAgentHookAdapter = _AgyAgentHookAdapter();

final class _AgyAgentHookAdapter extends _AgentHookAdapter {
  const _AgyAgentHookAdapter();

  @override
  AgentStatusState? normalizeState(
    AgentHookEvent event,
    String eventName,
    String? toolName,
    AgentStatusEntry? previous,
  ) => _normalizeAgyState(eventName, toolName, event.payload);

  @override
  bool isNewTurn(String eventName) => _isAgyNewTurn(eventName);

  @override
  String promptForEvent(AgentHookEvent event, String eventName) {
    final direct = super.promptForEvent(event, eventName);
    return direct.isNotEmpty ? direct : _agyPromptForEvent(event) ?? '';
  }

  @override
  _NestedToolCall nestedToolCall(Map<String, Object?> payload) {
    return _readAgyToolCall(payload);
  }

  @override
  AgentHookLifecyclePolicy createLifecyclePolicy() {
    return _AgyAgentHookLifecyclePolicy();
  }
}

final class _AgyAgentHookLifecyclePolicy implements AgentHookLifecyclePolicy {
  final Map<String, _AgyCompletedTurn> _completedTurns = {};

  @override
  bool shouldApply(AgentHookEvent event, String eventName) {
    final terminalSessionId = event.terminalSessionId;
    if (eventName == 'PreInvocation') {
      _completedTurns.remove(terminalSessionId);
      return true;
    }

    final completed = _completedTurns[terminalSessionId];
    if (completed != null && eventName != 'Stop') {
      final transcriptPath = _agyLifecycleString(event.payload, const [
        'transcriptPath',
        'transcript_path',
      ]);
      if (completed.transcriptPath == null ||
          transcriptPath == null ||
          completed.transcriptPath == transcriptPath) {
        return false;
      }
    }

    if (eventName == 'Stop' && !_agyStopStillBusy(event.payload)) {
      _completedTurns[terminalSessionId] = _AgyCompletedTurn(
        _agyLifecycleString(event.payload, const [
          'transcriptPath',
          'transcript_path',
        ]),
      );
    }
    return true;
  }

  @override
  void clearTerminal(String terminalSessionId) {
    _completedTurns.remove(terminalSessionId);
  }

  @override
  void reset() => _completedTurns.clear();
}

final class const _AgyCompletedTurn(final String? transcriptPath);

String? _agyLifecycleString(Map<String, Object?> payload, List<String> keys) {
  for (final key in keys) {
    final value = payload[key];
    if (value is String && value.trim().isNotEmpty) {
      return value.trim();
    }
  }
  return null;
}

AgentStatusState? _normalizeAgyState(
  String eventName,
  String? toolName,
  Map<String, Object?> payload,
) {
  // Alera never installs `PreToolUse`: Antigravity requires a `decision` there,
  // and an observational hook has no value to return that leaves the user's
  // permission policy alone. These branches only run for a hook the user wrote.
  if (eventName == 'PreToolUse' && _isAgyFeedbackTool(toolName)) {
    return AgentStatusState.waiting;
  }
  if (eventName == 'Stop' && _agyStopStillBusy(payload)) {
    return AgentStatusState.working;
  }
  return switch (eventName) {
    'PreInvocation' ||
    'PostInvocation' ||
    'PreToolUse' ||
    'PostToolUse' => AgentStatusState.working,
    'Stop' => AgentStatusState.done,
    _ => null,
  };
}

bool _agyStopStillBusy(Map<String, Object?> payload) {
  return payload['fullyIdle'] == false || payload['fully_idle'] == false;
}

bool _isAgyNewTurn(String eventName) {
  return eventName == 'PreInvocation';
}

String? _agyPromptForEvent(AgentHookEvent event) {
  return _readLastUserPromptFromTranscript(
    event.payload['transcriptPath'] ?? event.payload['transcript_path'],
  );
}

_NestedToolCall _readAgyToolCall(Map<String, Object?> payload) {
  final toolCall = payload['toolCall'];
  if (toolCall is! Map) {
    return const _NestedToolCall();
  }
  final record = Map<String, Object?>.from(toolCall);
  return _NestedToolCall(
    toolName: _readFirstString(record, const <String>[
      'name',
      'toolName',
      'tool_name',
    ]),
    toolInputSource: record['args'],
  );
}

bool _isAgyFeedbackTool(String? toolName) {
  return _isHumanInputTool(toolName);
}
