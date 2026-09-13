import 'dart:async';
import 'dart:convert';

import 'package:alera/src/features/agent_quota/application/agent_quota_providers.dart';
import 'package:alera/src/features/agent_quota/infra/runtime_proxy_client.dart';
import 'package:alera/src/features/remote_hosts/domain/ssh_target.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fake_recording_process_runner.dart';

void main() {
  test(
    'local quota consume reuses ids for retries and rotates after success',
    () async {
      final runtime = _RecordingRuntimeHostClient(<Future<Object?> Function()>[
        () => Future<Object?>.error(StateError('temporary failure')),
        () => Future<Object?>.value(_consumeResponse()),
        () => Future<Object?>.value(_consumeResponse()),
      ]);
      final service = AgentQuotaService(
        RuntimeProxyClient(
          processRunner: FakeRecordingProcessRunner(<Object>[]),
        ),
        runtime,
      );

      await expectLater(
        service.consumeCodexResetCredit(
          hostId: 'local',
          target: null,
          offerRevision: 'offer-1',
        ),
        throwsA(isA<StateError>()),
      );
      await service.consumeCodexResetCredit(
        hostId: 'local',
        target: null,
        offerRevision: 'offer-1',
      );
      await service.consumeCodexResetCredit(
        hostId: 'local',
        target: null,
        offerRevision: 'offer-2',
      );

      expect(runtime.payloads, hasLength(3));
      expect(runtime.payloads[0]['offerRevision'], 'offer-1');
      expect(runtime.payloads[0]['clientMutationId'], isA<String>());
      expect(
        runtime.payloads[1]['clientMutationId'],
        runtime.payloads[0]['clientMutationId'],
      );
      expect(
        runtime.payloads[2]['clientMutationId'],
        isNot(runtime.payloads[1]['clientMutationId']),
      );
    },
  );

  test(
    'remote quota consume sends the mutation id through the proxy',
    () async {
      final runner = _RecordingRuntimeProxyProcessRunner(<Map<String, Object?>>[
        <String, Object?>{'id': 1, 'ok': false, 'error': 'temporary failure'},
        <String, Object?>{'id': 1, 'ok': true, 'payload': _consumeResponse()},
        <String, Object?>{'id': 1, 'ok': true, 'payload': _consumeResponse()},
      ]);
      final service = AgentQuotaService(
        RuntimeProxyClient(processRunner: runner),
      );

      await expectLater(
        service.consumeCodexResetCredit(
          hostId: 'remote',
          target: _target(),
          offerRevision: 'offer-1',
        ),
        throwsA(isA<StateError>()),
      );
      await service.consumeCodexResetCredit(
        hostId: 'remote',
        target: _target(),
        offerRevision: 'offer-1',
      );
      await service.consumeCodexResetCredit(
        hostId: 'remote',
        target: _target(),
        offerRevision: 'offer-2',
      );

      expect(runner.requests, hasLength(3));
      final payloads = <Map<String, Object?>>[
        for (final request in runner.requests)
          Map<String, Object?>.from(request['payload']! as Map),
      ];
      expect(payloads[0]['clientMutationId'], isA<String>());
      expect(payloads[1]['clientMutationId'], payloads[0]['clientMutationId']);
      expect(
        payloads[2]['clientMutationId'],
        isNot(payloads[1]['clientMutationId']),
      );
    },
  );
}

Map<String, Object?> _consumeResponse() {
  return <String, Object?>{
    'status': 'consumed',
    'outcome': 'reset',
    'snapshot': <String, Object?>{
      'provider': 'codex',
      'accountId': 'default',
      'displayName': 'Codex',
      'status': 'ok',
      'updatedAt': 0,
      'windows': <Object?>[],
      'buckets': <Object?>[],
    },
  };
}

final class _RecordingRuntimeHostClient implements RuntimeHostClient {
  _RecordingRuntimeHostClient(this._responses);

  final List<Future<Object?> Function()> _responses;
  final List<Map<String, Object?>> payloads = <Map<String, Object?>>[];

  @override
  Stream<RuntimeHostEvent> get runtimeEvents => const Stream.empty();

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const <String, Object?>{},
    Duration? timeout,
  ]) {
    expect(type, 'agentQuota.consumeCodexResetCredit');
    payloads.add(payload);
    return _responses.removeAt(0)();
  }
}

final class _RecordingRuntimeProxyProcessRunner implements ProcessRunner {
  _RecordingRuntimeProxyProcessRunner(this._responses);

  final List<Map<String, Object?>> _responses;
  final List<Map<String, Object?>> requests = <Map<String, Object?>>[];

  @override
  Future<StartedProcess> start(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
    bool includeParentEnvironment = true,
  }) async {
    final response = _responses.removeAt(0);
    return StartedProcess(
      stdinWrite: (data) {
        requests.add(
          Map<String, Object?>.from(jsonDecode(utf8.decode(data)) as Map),
        );
      },
      stdout: Stream<List<int>>.value(utf8.encode('${jsonEncode(response)}\n')),
      stderr: const Stream<List<int>>.empty(),
      pid: 1,
      exitCode: Future<int>.value(0),
      kill: ([signal]) => true,
    );
  }

  @override
  Future<ProcessRunOutput> run(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
  }) {
    throw UnimplementedError();
  }
}

SshTarget _target() {
  final now = DateTime.utc(2026);
  return SshTarget(
    id: 'remote',
    alias: 'remote',
    host: 'example.test',
    port: 22,
    username: 'user',
    authKind: .key,
    createdAt: now,
    updatedAt: now,
    runtimePlatform: 'linux',
    bootstrapStatus: .installed,
  );
}
