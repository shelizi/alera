part of '../agent_hook_event_normalizer.dart';

const _fxAgentHookAdapter = _FxAgentHookAdapter();

final class _FxAgentHookAdapter extends _AgentHookAdapter {
  const _FxAgentHookAdapter();

  @override
  AgentStatusState? normalizeState(
    AgentHookEvent event,
    String eventName,
    String? toolName,
    AgentStatusEntry? previous,
  ) {
    return switch (eventName) {
      'Working' => AgentStatusState.working,
      'Blocked' => AgentStatusState.blocked,
      'Idle' => AgentStatusState.done,
      _ => null,
    };
  }

  @override
  bool isNewTurn(String eventName) => eventName == 'Working';

  @override
  bool isSessionClose(String eventName) => eventName == 'SessionEnd';
}
