import 'dart:async';

import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/application/runtime_agent_status_sync.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:alera/src/shared/infra/runtime/runtime_change_coalescer.dart';
import 'package:alera/src/shared/infra/runtime/runtime_host_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
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

  test(
    'an in-flight snapshot cannot overwrite a newer runtime delta',
    () async {
      final client = _BlockingRuntimeHostClient();
      final coalescer = RuntimeChangeCoalescer(
        debounce: Duration.zero,
        maxDelay: Duration.zero,
      );
      final container = ProviderContainer(
        overrides: [
          runtimeHostClientProvider.overrideWithValue(client),
          runtimeChangeCoalescerProvider.overrideWithValue(coalescer),
        ],
      );
      addTearDown(() async {
        container.dispose();
        coalescer.dispose();
        await client.close();
      });

      container.read(runtimeAgentStatusSyncProvider);
      await client.snapshotRequested.future;

      client.emit(
        RuntimeHostEvent('agentPresenceChanged', <String, Object?>{
          'changes': <Object?>[
            _presencePayload(
              state: 'waiting',
              updatedAt: '2026-09-10T08:00:02Z',
            ),
          ],
        }),
      );
      await Future<void>.delayed(Duration.zero);

      expect(
        container.read(agentStatusControllerProvider)['session-1']?.state,
        AgentStatusState.waiting,
      );

      client.snapshotResponse.complete(<Object?>[
        _presencePayload(state: 'working', updatedAt: '2026-09-10T08:00:01Z'),
      ]);
      await Future<void>.delayed(Duration.zero);
      await Future<void>.delayed(Duration.zero);

      expect(
        container.read(agentStatusControllerProvider)['session-1']?.state,
        AgentStatusState.waiting,
        reason: 'the older snapshot request started before the delta arrived',
      );
    },
  );
}

Map<String, Object?> _presencePayload({
  required String state,
  required String updatedAt,
}) {
  return <String, Object?>{
    'terminalSessionId': 'session-1',
    'workspaceId': 'workspace-1',
    'tabId': 'tab-1',
    'agentType': 'codex',
    'agentState': state,
    'prompt': '',
    'updatedAt': updatedAt,
    'stateStartedAt': updatedAt,
  };
}

final class _BlockingRuntimeHostClient implements RuntimeHostClient {
  final events = StreamController<RuntimeHostEvent>.broadcast(sync: true);
  final snapshotRequested = Completer<void>();
  final snapshotResponse = Completer<Object?>();

  @override
  Stream<RuntimeHostEvent> get runtimeEvents => events.stream;

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const <String, Object?>{},
    Duration? timeout,
  ]) async {
    if (type != 'agentPresence.list') {
      return null;
    }
    if (!snapshotRequested.isCompleted) {
      snapshotRequested.complete();
    }
    return snapshotResponse.future;
  }

  void emit(RuntimeHostEvent event) => events.add(event);

  Future<void> close() => events.close();
}
