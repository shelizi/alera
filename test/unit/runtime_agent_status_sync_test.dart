import 'package:alera/src/features/agent_status/application/runtime_agent_status_sync.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('decodes direct runtime presence changes for background sessions', () {
    final delta = decodeRuntimeAgentPresenceDelta(<String, Object?>{
      'changes': <Object?>[
        <String, Object?>{
          'terminalSessionId': 'session-2',
          'workspaceId': 'workspace-1',
          'tabId': 'tab-2',
          'agentType': 'codex',
          'state': 'waiting',
          'stateStartedAt': '2026-09-09T13:00:00Z',
          'updatedAt': '2026-09-09T13:00:01Z',
          'prompt': 'Need input',
        },
        <String, Object?>{'terminalSessionId': 'session-3', 'removed': true},
      ],
    });

    expect(delta, isNotNull);
    expect(delta!.upserts, hasLength(1));
    expect(delta.upserts.single.terminalSessionId, 'session-2');
    expect(delta.upserts.single.state, AgentStatusState.waiting);
    expect(delta.removedSessionIds, <String>{'session-3'});
  });

  test('legacy empty event requests snapshot fallback', () {
    expect(decodeRuntimeAgentPresenceDelta(const <String, Object?>{}), isNull);
  });
}
