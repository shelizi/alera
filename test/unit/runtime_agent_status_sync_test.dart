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
          'contextOverview': <String, Object?>{
            'conversationId': 'activation-b',
            'goal': '將 Agent 的整體任務進度同步到側邊欄',
            'progress': '完成 Hook 整合',
            'timeline': <Object?>[
              <String, Object?>{
                'kind': 'report',
                'text': '完成 Hook 整合',
                'at': '2026-09-09T13:00:01Z',
              },
            ],
          },
        },
        <String, Object?>{'terminalSessionId': 'session-3', 'removed': true},
      ],
    });

    expect(delta, isNotNull);
    expect(delta!.upserts, hasLength(1));
    expect(delta.upserts.single.terminalSessionId, 'session-2');
    expect(delta.upserts.single.state, AgentStatusState.waiting);
    expect(
      delta.upserts.single.contextOverview?['goal'],
      '將 Agent 的整體任務進度同步到側邊欄',
    );
    expect(
      delta.upserts.single.contextOverview?['conversationId'],
      'activation-b',
    );
    final timeline = delta.upserts.single.contextOverview?['timeline'] as List;
    expect(timeline, hasLength(1));
    expect(delta.removedSessionIds, <String>{'session-3'});
  });

  test(
    'repeated full Host Context reuses validated data, not contextChanged hint',
    () {
      Map<String, Object?> change(
        int index, {
        String goal = 'Current goal',
        String workspaceId = 'workspace-1',
        String? report,
        String? conversationId,
      }) => {
        ..._presencePayload(
          state: 'working',
          updatedAt: '2026-09-10T08:00:02Z',
        ),
        'workspaceId': workspaceId,
        'toolName': 'Tool $index',
        'contextChanged': false,
        'contextOverview': <String, Object?>{
          if (conversationId != null) 'conversationId': conversationId,
          'goal': goal,
          'progress': 'Progress report',
          'reportSummary': report,
          'initialGoal': 'Initial goal',
          'currentFocus': 'Focus',
          'focusUpdatedAt': '2026-09-10T08:00:00+00:00',
          'timeline': [
            for (var i = 0; i < 16; i++)
              {
                'kind': 'report',
                'text': 'Event $i',
                'at': '2026-09-10T08:00:00+00:00',
              },
          ],
        },
      };
      AgentStatusEntry decode(
        Map<String, Object?> payload, [
        AgentStatusEntry? old,
      ]) => decodeRuntimeAgentPresenceDelta(
        {
          'changes': [payload],
        },
        previous: old == null ? {} : {old.terminalSessionId: old},
      )!.upserts.single;
      final first = decode(change(0));
      var latest = first;
      for (var i = 1; i <= 1000; i++) {
        latest = decode(change(i), latest);
        expect(latest.contextOverview, same(first.contextOverview));
        expect(latest.toolName, 'Tool $i');
      }
      expect(decode(change(1000), latest), same(latest));
      // Content and timestamps can be identical across activations. The
      // Host token must survive validation without reusing another Session.
      final a = decode(change(1000, conversationId: 'activation-a'), latest);
      expect(a.contextOverview?['conversationId'], 'activation-a');
      expect(a.contextOverview, isNot(same(latest.contextOverview)));
      expect(decode(change(1000, conversationId: 'activation-a'), a), same(a));
      final b = decode(change(1000, conversationId: 'activation-b'), a);
      expect(b.contextOverview?['conversationId'], 'activation-b');
      expect(b.contextOverview, isNot(same(a.contextOverview)));
      expect(a.contextOverview?['conversationId'], 'activation-a');
      expect(
        () => b.contextOverview!['conversationId'] = 'changed',
        throwsUnsupportedError,
      );
      expect(decode(change(1000, conversationId: 'activation-b'), b), same(b));
      final unbound = decode(change(1000), b);
      expect(unbound.contextOverview, isNot(contains('conversationId')));
      final malformed = decode(change(1000, conversationId: 'x' * 129), b);
      expect(malformed.contextOverview, isNot(contains('conversationId')));
      final updated = decode(change(1001, report: 'New AI summary'), latest);
      expect(updated.contextOverview, isNot(same(first.contextOverview)));
      expect(updated.contextOverview!['reportSummary'], 'New AI summary');
      final moved = decode(change(1001, workspaceId: 'workspace-2'), latest);
      expect(moved.contextOverview, isNot(same(first.contextOverview)));
      final changed = decode(change(1002, goal: 'Different task'), updated);
      expect(changed.contextOverview!['goal'], 'Different task');
      final raw = change(1003);
      final immutable = decode(raw);
      final context = raw['contextOverview'] as Map;
      (context['timeline'] as List).first['text'] = 'Changed timeline';
      final mutated = decode(raw, immutable);
      expect(mutated.contextOverview, isNot(same(immutable.contextOverview)));
      expect(
        (immutable.contextOverview!['timeline'] as List).first['text'],
        'Event 0',
      );
      expect(
        (mutated.contextOverview!['timeline'] as List).first['text'],
        'Changed timeline',
      );
      context['timeline'] = [
        {'kind': 'report', 'text': 'x' * 10000, 'at': '2026-09-10T08:00:00Z'},
      ];
      final bounded = decode(raw, mutated);
      expect(
        ((bounded.contextOverview!['timeline'] as List).single['text']
                as String)
            .length,
        260,
      );
      expect(
        decode(raw, bounded).contextOverview,
        same(bounded.contextOverview),
      );
      final absent = decode({...change(1004), 'contextOverview': null}, latest);
      expect(absent.contextOverview, isNull);
    },
  );

  test('reconnect bursts keep one full presence RPC in flight', () async {
    final client = _QueuedRuntimeHostClient();
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
      await client.events.close();
    });
    container.read(runtimeAgentStatusSyncProvider);
    await Future<void>.delayed(Duration.zero);
    expect(client.requests, hasLength(1));

    for (var i = 0; i < 100; i++) {
      client.events.add(
        const RuntimeHostEvent(aleraRuntimeHostConnectedEvent, {}),
      );
      client.events.add(const RuntimeHostEvent('agentPresenceChanged', {}));
    }
    await Future<void>.delayed(Duration.zero);
    expect(client.requests, hasLength(1));
    expect(client.maximumInFlight, 1);

    client.requests.first.complete([
      _presencePayload(state: 'working', updatedAt: '2026-09-10T08:00:00Z'),
    ]);
    await Future<void>.delayed(Duration.zero);
    expect(client.requests, hasLength(2));
    expect(container.read(agentStatusControllerProvider), isEmpty);

    client.requests.last.complete([
      _presencePayload(state: 'waiting', updatedAt: '2026-09-10T08:00:02Z'),
    ]);
    await Future<void>.delayed(Duration.zero);
    final snapshot = container.read(agentStatusControllerProvider);
    expect(snapshot['session-1']?.state, AgentStatusState.waiting);
    expect(client.maximumInFlight, 1);
    expect(client.requests, hasLength(2));

    client.events.add(const RuntimeHostEvent('agentPresenceChanged', {}));
    await Future<void>.delayed(Duration.zero);
    expect(client.requests, hasLength(3));
    client.requests.last.complete([
      _presencePayload(state: 'waiting', updatedAt: '2026-09-10T08:00:02Z'),
    ]);
    await Future<void>.delayed(Duration.zero);
    expect(container.read(agentStatusControllerProvider), same(snapshot));
    expect(client.maximumInFlight, 1);
  });

  test(
    'legacy invalidations during a full read queue one fresh snapshot',
    () async {
      final client = _QueuedRuntimeHostClient();
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
        await client.events.close();
      });
      container.read(runtimeAgentStatusSyncProvider);
      await Future<void>.delayed(Duration.zero);
      client.events.add(const RuntimeHostEvent('agentPresenceChanged', {}));
      await Future<void>.delayed(Duration.zero);
      expect(client.requests, hasLength(1));
      client.requests.first.complete([
        _presencePayload(state: 'done', updatedAt: '2026-09-10T08:00:00Z'),
      ]);
      await Future<void>.delayed(Duration.zero);
      expect(container.read(agentStatusControllerProvider), isEmpty);
      expect(client.requests, hasLength(2));
      client.requests.last.complete([
        _presencePayload(state: 'working', updatedAt: '2026-09-10T08:00:01Z'),
      ]);
      await Future<void>.delayed(Duration.zero);
      expect(
        container.read(agentStatusControllerProvider)['session-1']?.state,
        AgentStatusState.working,
      );
      expect(client.maximumInFlight, 1);
    },
  );

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

final class _QueuedRuntimeHostClient implements RuntimeHostClient {
  final events = StreamController<RuntimeHostEvent>.broadcast(sync: true);
  final requests = <Completer<Object?>>[];
  var inFlight = 0;
  var maximumInFlight = 0;

  @override
  Stream<RuntimeHostEvent> get runtimeEvents => events.stream;

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const {},
    Duration? timeout,
  ]) async {
    if (type != 'agentPresence.list') return null;
    final request = Completer<Object?>();
    requests.add(request);
    inFlight++;
    if (inFlight > maximumInFlight) maximumInFlight = inFlight;
    try {
      return await request.future;
    } finally {
      inFlight--;
    }
  }
}
