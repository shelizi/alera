import 'package:alera/src/features/agent_quota/application/agent_quota_providers.dart';
import 'package:alera/src/features/agent_quota/domain/agent_quota.dart';
import 'package:alera/src/features/agent_quota/infra/runtime_proxy_client.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fake_recording_process_runner.dart';

void main() {
  test('refreshing one agent keeps the other snapshots', () async {
    final now = DateTime.now().toUtc().millisecondsSinceEpoch;
    final client = _ScriptedQuotaClient(<Object?>[
      <String, Object?>{
        'snapshots': <Object?>[
          _row('claude', 'Claude', now),
          _row('codex', 'Codex', now),
        ],
        'environment': <String, bool>{'KIMI_API_KEY': true},
      },
      <String, Object?>{
        'snapshots': <Object?>[_row('codex', 'Codex Updated', now)],
        'environment': <String, bool>{'KIMI_API_KEY': true},
      },
    ]);
    final service = AgentQuotaService(
      RuntimeProxyClient(processRunner: FakeRecordingProcessRunner(<Object>[])),
      client,
    );

    final initial = await service.fetch(
      hostId: 'local',
      target: null,
      settings: .defaults,
    );
    final refreshed = await service.refreshSnapshot(
      hostId: 'local',
      target: null,
      settings: .defaults,
      provider: .codex,
      accountId: 'default',
    );

    expect(client.requests, hasLength(2));
    expect(client.requests.first['provider'], isNull);
    expect(client.requests.last['type'], 'agentQuota.snapshot');
    expect(client.requests.last['forceRefresh'], isTrue);
    expect(client.requests.last['provider'], 'codex');
    expect(client.requests.last['accountId'], 'default');
    expect(initial.snapshots.map((snapshot) => snapshot.provider), [
      AgentQuotaProviderId.claude,
      AgentQuotaProviderId.codex,
    ]);
    expect(refreshed.snapshots, hasLength(2));
    expect(refreshed.snapshots.first.displayName, 'Claude');
    expect(refreshed.snapshots.first.status, AgentQuotaStatus.ok);
    expect(refreshed.snapshots.last.displayName, 'Codex Updated');
    expect(refreshed.snapshots.last.status, AgentQuotaStatus.ok);
  });

  test('a failed agent refresh marks only that snapshot stale', () async {
    final now = DateTime.now().toUtc().millisecondsSinceEpoch;
    final client = _ScriptedQuotaClient(<Object?>[
      <String, Object?>{
        'snapshots': <Object?>[
          _row('claude', 'Claude', now),
          _row('codex', 'Codex', now),
        ],
      },
      StateError('codex offline'),
    ]);
    final service = AgentQuotaService(
      RuntimeProxyClient(processRunner: FakeRecordingProcessRunner(<Object>[])),
      client,
    );
    await service.fetch(hostId: 'local', target: null, settings: .defaults);

    final refreshed = await service.refreshSnapshot(
      hostId: 'local',
      target: null,
      settings: .defaults,
      provider: .codex,
      accountId: 'default',
    );

    expect(refreshed.error, isNull);
    expect(refreshed.snapshots.first.status, AgentQuotaStatus.ok);
    expect(refreshed.snapshots.last.status, AgentQuotaStatus.stale);
    expect(refreshed.snapshots.last.error, contains('codex offline'));
    expect(refreshed.snapshots.last.displayName, 'Codex');
  });
}

Map<String, Object?> _row(String provider, String displayName, int updatedAt) {
  return <String, Object?>{
    'provider': provider,
    'accountId': 'default',
    'displayName': displayName,
    'status': 'ok',
    'updatedAt': updatedAt,
  };
}

final class _ScriptedQuotaClient implements RuntimeHostClient {
  _ScriptedQuotaClient(this._responses);

  final List<Object?> _responses;
  final List<Map<String, Object?>> requests = <Map<String, Object?>>[];
  var _index = 0;

  @override
  Stream<RuntimeHostEvent> get runtimeEvents => const Stream.empty();

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const <String, Object?>{},
    Duration? timeout,
  ]) async {
    requests.add(<String, Object?>{'type': type, ...payload});
    final response = _responses[_index++];
    if (response is StateError) {
      throw response;
    }
    return response;
  }
}
