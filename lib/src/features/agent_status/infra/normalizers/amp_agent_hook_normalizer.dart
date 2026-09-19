part of '../agent_hook_event_normalizer.dart';

const _ampAgentHookAdapter = _AmpAgentHookAdapter();

final class _AmpAgentHookAdapter extends _AgentHookAdapter {
  const _AmpAgentHookAdapter();

  @override
  AgentStatusState? normalizeState(
    AgentHookEvent event,
    String eventName,
    String? toolName,
    AgentStatusEntry? previous,
  ) => _normalizeAmpState(eventName);

  @override
  bool isNewTurn(String eventName) => _isAmpNewTurn(eventName);

  @override
  String? assistantText(AgentHookEvent event, String eventName) {
    return _ampAssistantTextForEvent(event, eventName);
  }

  @override
  bool isInterrupted(AgentHookEvent event, String eventName) {
    return _isAmpInterrupted(event);
  }

  @override
  AgentHookLifecyclePolicy createLifecyclePolicy() {
    return _AmpAgentHookLifecyclePolicy();
  }
}

final class _AmpAgentHookLifecyclePolicy implements AgentHookLifecyclePolicy {
  static const _completedThreadLimit = 32;
  static const _legacyThreadKey = '<legacy>';

  final Map<String, _AmpTerminalLifecycle> _terminals = {};

  @override
  bool shouldApply(AgentHookEvent event, String eventName) {
    final terminal = _terminals.putIfAbsent(
      event.terminalSessionId,
      _AmpTerminalLifecycle.new,
    );
    final threadKey = _threadKey(event.payload);
    if (eventName == 'session.start') {
      terminal.completedThreads.remove(threadKey);
      return false;
    }
    if (eventName == 'agent.start') {
      terminal.completedThreads.remove(threadKey);
      return true;
    }
    if ((eventName == 'tool.call' || eventName == 'tool.result') &&
        terminal.completedThreads.contains(threadKey)) {
      return false;
    }
    if (eventName == 'agent.end') {
      terminal.completedThreads
        ..remove(threadKey)
        ..add(threadKey);
      while (terminal.completedThreads.length > _completedThreadLimit) {
        terminal.completedThreads.remove(terminal.completedThreads.first);
      }
    }
    return true;
  }

  @override
  void clearTerminal(String terminalSessionId) {
    _terminals.remove(terminalSessionId);
  }

  @override
  void reset() => _terminals.clear();

  String _threadKey(Map<String, Object?> payload) {
    final direct = _readThreadString(payload, const [
      'threadId',
      'threadID',
      'thread_id',
    ]);
    if (direct != null) {
      return direct;
    }
    final thread = payload['thread'];
    if (thread is Map) {
      final nested = _readThreadString(
        Map<String, Object?>.from(thread),
        const ['id'],
      );
      if (nested != null) {
        return nested;
      }
    }
    return _legacyThreadKey;
  }

  String? _readThreadString(Map<String, Object?> payload, List<String> keys) {
    for (final key in keys) {
      final value = payload[key];
      if (value is String && value.trim().isNotEmpty) {
        return value.trim();
      }
    }
    return null;
  }
}

final class _AmpTerminalLifecycle {
  final LinkedHashSet<String> completedThreads = LinkedHashSet<String>();
}

AgentStatusState? _normalizeAmpState(String eventName) {
  return switch (eventName) {
    'agent.start' || 'tool.call' || 'tool.result' => AgentStatusState.working,
    'agent.end' => AgentStatusState.done,
    _ => null,
  };
}

bool _isAmpNewTurn(String eventName) {
  return eventName == 'agent.start';
}

String? _ampAssistantTextForEvent(AgentHookEvent event, String eventName) {
  if (eventName != 'agent.end') {
    return null;
  }
  return _lastAmpAssistantMessage(event.payload['messages']);
}

String? _lastAmpAssistantMessage(Object? value) {
  if (value is! List) {
    return null;
  }
  for (final message in value.reversed) {
    if (message is! Map) {
      continue;
    }
    final record = Map<String, Object?>.from(message);
    if (record['role'] != 'assistant') {
      continue;
    }
    final text = _ampMessageText(record['content']);
    if (text != null) {
      return text;
    }
  }
  return null;
}

String? _ampMessageText(Object? value) {
  if (value is String && value.trim().isNotEmpty) {
    return _normalizeMultiline(value, 8000);
  }
  if (value is! List) {
    return null;
  }
  final out = StringBuffer();
  for (final part in value) {
    if (part is! Map) {
      continue;
    }
    final record = Map<String, Object?>.from(part);
    if (record['type'] == 'text') {
      final text = record['text'];
      if (text is String && text.isNotEmpty) {
        out.write(text);
      }
    }
  }
  final normalized = out.toString().trim();
  return normalized.isEmpty ? null : _normalizeMultiline(normalized, 8000);
}

bool _isAmpInterrupted(AgentHookEvent event) {
  return _isGenericInterrupted(event) || event.payload['status'] == 'cancelled';
}
