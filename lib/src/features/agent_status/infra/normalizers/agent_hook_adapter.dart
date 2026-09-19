part of '../agent_hook_event_normalizer.dart';

/// Stateful lifecycle filter owned by one agent adapter.
///
/// Most agents use the pass-through implementation. Agents with ordering or
/// dedupe semantics (currently AGY and Amp) keep that state in their own
/// adapter file instead of teaching the shared controller about the agent.
abstract interface class AgentHookLifecyclePolicy {
  bool shouldApply(AgentHookEvent event, String eventName);

  void clearTerminal(String terminalSessionId);

  void reset();
}

final class _PassThroughAgentHookLifecyclePolicy
    implements AgentHookLifecyclePolicy {
  const _PassThroughAgentHookLifecyclePolicy();

  @override
  bool shouldApply(AgentHookEvent event, String eventName) => true;

  @override
  void clearTerminal(String terminalSessionId) {}

  @override
  void reset() {}
}

abstract base class _AgentHookAdapter {
  const _AgentHookAdapter();

  String? normalizeEventName(AgentHookEvent event, String? rawEventName) {
    return rawEventName;
  }

  AgentStatusState? normalizeState(
    AgentHookEvent event,
    String eventName,
    String? toolName,
    AgentStatusEntry? previous,
  ) {
    return null;
  }

  bool isNewTurn(String eventName) => false;

  bool isSessionClose(String eventName) => false;

  bool isSessionReset(String eventName) => false;

  String promptForEvent(AgentHookEvent event, String eventName) {
    return _extractPrompt(event.payload);
  }

  bool isInterrupted(AgentHookEvent event, String eventName) {
    return _isGenericInterrupted(event);
  }

  bool? interruptedFor(
    AgentHookEvent event,
    String eventName,
    AgentStatusState state,
    AgentStatusEntry? previous,
  ) {
    if (state != AgentStatusState.done) {
      return null;
    }
    return isInterrupted(event, eventName) ? true : null;
  }

  bool isExplicitInterrupt(AgentHookEvent event, String eventName) {
    return _isExplicitInterruptPayload(event, eventName);
  }

  bool shouldTakeOverActiveStatusFrom(AgentType previousAgentType) {
    return previousAgentType.key == 'claude';
  }

  bool isToolEvent(String eventName) {
    return const <String>{
      'PreToolUse',
      'PostToolUse',
      'PostToolUseFailure',
      'PermissionRequest',
      'tool_call',
      'tool_execution_start',
      'tool_execution_end',
      'tool.call',
      'tool.result',
    }.contains(eventName);
  }

  _ToolSnapshot? extractToolSnapshot(AgentHookEvent event, String eventName) {
    return null;
  }

  _NestedToolCall nestedToolCall(Map<String, Object?> payload) {
    return const _NestedToolCall();
  }

  String? assistantText(AgentHookEvent event, String eventName) => null;

  bool shouldReadAssistantTranscript(String eventName) => eventName == 'Stop';

  AgentHookLifecyclePolicy createLifecyclePolicy() {
    return const _PassThroughAgentHookLifecyclePolicy();
  }
}

final Map<String, _AgentHookAdapter> _agentHookAdapters =
    <String, _AgentHookAdapter>{
      'codex': _codexAgentHookAdapter,
      'claude': _claudeAgentHookAdapter,
      'copilot': _copilotAgentHookAdapter,
      'cursor': _cursorAgentHookAdapter,
      'agy': _agyAgentHookAdapter,
      'opencode': _openCodeAgentHookAdapter,
      'opencode2': _openCode2AgentHookAdapter,
      'pi': _piAgentHookAdapter,
      'amp': _ampAgentHookAdapter,
      'grok': _grokAgentHookAdapter,
      'devin': _devinAgentHookAdapter,
      'fx': _fxAgentHookAdapter,
    };

_AgentHookAdapter _agentHookAdapterFor(AgentType agentType) {
  final adapter = _agentHookAdapters[agentType.key];
  if (adapter == null) {
    throw StateError('Missing hook adapter for ${agentType.key}.');
  }
  return adapter;
}

AgentHookLifecyclePolicy createAgentHookLifecyclePolicy(AgentType agentType) {
  return _agentHookAdapterFor(agentType).createLifecyclePolicy();
}

bool shouldAgentHookTakeOverActiveStatus({
  required AgentType incomingAgentType,
  required AgentType previousAgentType,
}) {
  return _agentHookAdapterFor(incomingAgentType)
      .shouldTakeOverActiveStatusFrom(previousAgentType);
}

bool _isExplicitInterruptPayload(AgentHookEvent event, String eventName) {
  if (eventName == 'Interrupt') {
    return true;
  }
  if (event.payload['is_interrupt'] == true ||
      event.payload['interrupted'] == true) {
    return true;
  }
  final status = _readFirstString(event.payload, const <String>[
    'status',
  ])?.toLowerCase();
  return status == 'interrupted' ||
      status == 'cancelled' ||
      status == 'canceled' ||
      status == 'aborted';
}
