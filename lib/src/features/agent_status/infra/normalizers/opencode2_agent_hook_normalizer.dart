part of '../agent_hook_event_normalizer.dart';

const _openCode2AgentHookAdapter = _OpenCode2AgentHookAdapter();

/// OpenCode 2 currently speaks the same status protocol as OpenCode, but owns
/// a distinct adapter so either protocol can diverge without editing the other.
final class _OpenCode2AgentHookAdapter extends _AgentHookAdapter {
  const _OpenCode2AgentHookAdapter();

  @override
  AgentStatusState? normalizeState(
    AgentHookEvent event,
    String eventName,
    String? toolName,
    AgentStatusEntry? previous,
  ) => _normalizeOpenCodeState(eventName);

  @override
  bool isNewTurn(String eventName) => _isOpenCodeNewTurn(eventName);

  @override
  String promptForEvent(AgentHookEvent event, String eventName) {
    return _openCodePromptForEvent(event, eventName) ??
        super.promptForEvent(event, eventName);
  }

  @override
  String? assistantText(AgentHookEvent event, String eventName) {
    return _openCodeAssistantTextForEvent(event, eventName);
  }
}
