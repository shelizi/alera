part of 'agent_hook_event_normalizer_test.dart';

void _registerDevinAgentHookEventNormalizerTests() {
  test('normalizes Devin lifecycle and official tool payload fields', () {
    final prompt = normalizeAgentHookEvent(
      _event(
        agentType: .devin,
        hookEventName: 'UserPromptSubmit',
        payload: const <String, Object?>{'prompt': 'Ship the feature'},
      ),
    );
    expect(prompt?.state, AgentStatusState.working);
    expect(prompt?.prompt, 'Ship the feature');

    final tool = normalizeAgentHookEvent(
      _event(
        agentType: .devin,
        hookEventName: 'PreToolUse',
        payload: const <String, Object?>{
          'tool_name': 'Shell',
          'tool_input': <String, Object?>{'command': 'dart test'},
        },
      ),
    );
    expect(tool?.state, AgentStatusState.working);
    expect(tool?.toolName, 'Shell');
    expect(tool?.toolInput, 'dart test');

    expect(
      normalizeAgentHookEvent(
        _event(
          agentType: .devin,
          hookEventName: 'PermissionRequest',
          payload: const <String, Object?>{},
        ),
      )?.state,
      AgentStatusState.blocked,
    );
    expect(
      normalizeAgentHookEvent(
        _event(
          agentType: .claude,
          hookEventName: 'PermissionRequest',
          payload: const <String, Object?>{},
        ),
      )?.state,
      AgentStatusState.waiting,
      reason: 'Devin policy must not leak into Claude',
    );
    expect(
      normalizeAgentHookEvent(
        _event(
          agentType: .devin,
          hookEventName: 'Stop',
          payload: const <String, Object?>{},
        ),
      )?.state,
      AgentStatusState.done,
    );
    expect(
      isAgentSessionCloseHookEvent(
        _event(
          agentType: .devin,
          hookEventName: 'SessionEnd',
          payload: const <String, Object?>{},
        ),
      ),
      isTrue,
    );
  });
}
