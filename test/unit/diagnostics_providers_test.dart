import 'dart:async';

import 'package:alera/src/features/diagnostics/application/diagnostics_providers.dart';
import 'package:alera/src/features/runtime_host/application/runtime_host_lifecycle_providers.dart';
import 'package:alera/src/features/runtime_host/application/runtime_host_lifecycle_service.dart';
import 'package:alera/src/features/runtime_host/domain/runtime_host_status.dart';
import 'package:alera/src/features/settings/application/runtime_settings_changes.dart';
import 'package:alera/src/features/settings/application/settings_providers.dart';
import 'package:alera/src/features/settings/application/settings_repository.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/shared/infra/storage/drift_database.dart';
import 'package:alera/src/shared/infra/storage/storage_providers.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:drift/native.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('waits for the database before reading settings', () async {
    final databaseCompleter = Completer<AleraDatabase>();
    final database = AleraDatabase(executor: NativeDatabase.memory());
    var repositoryCreated = false;
    final container = ProviderContainer(
      overrides: [
        aleraDatabaseProvider.overrideWith((ref) => databaseCompleter.future),
        settingsRepositoryProvider.overrideWith((ref) {
          repositoryCreated = true;
          return _FakeSettingsRepository();
        }),
        runtimeSettingsChangesProvider.overrideWith(
          (ref) => const Stream<void>.empty(),
        ),
      ],
    );

    container.read(diagnosticsSettingsApplierProvider);
    expect(repositoryCreated, isFalse);

    databaseCompleter.complete(database);
    await container.read(aleraDatabaseProvider.future);
    container.read(diagnosticsSettingsApplierProvider);

    expect(repositoryCreated, isTrue);

    container.dispose();
    await database.close();
  });

  test(
    'runtime diagnostics reads facts through lifecycle probe contract',
    () async {
      final client = _FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.4.0',
          'runtimeHostCommit': 'abc123',
          'protocolVersion': 4,
          'logDirectory': r'C:\\logs\\alera',
          'runtimeCapabilities': <Object?>[
            'runtimeStore',
            'hostDiagnosticsLogsV1',
          ],
        },
      );
      final container = ProviderContainer(
        overrides: [
          runtimeHostLifecycleClientProvider.overrideWithValue(client),
        ],
      );
      addTearDown(container.dispose);

      final info = await container.read(runtimeDiagnosticsInfoProvider.future);

      expect(info.version, '1.4.0');
      expect(info.commit, 'abc123');
      expect(info.protocolVersion, 4);
      expect(info.logDirectory, r'C:\\logs\\alera');
      expect(info.capabilities, <String>[
        'runtimeStore',
        'hostDiagnosticsLogsV1',
      ]);
      expect(client.probeCalls, 1);
    },
  );
}

final class _FakeRuntimeHostLifecycleClient({this.status})
    implements RuntimeHostLifecycleClient {
  final Map<String, Object?>? status;
  int probeCalls = 0;

  @override
  void beginAppQuit() {}

  @override
  void cancelAppQuit() {}

  @override
  void commitAppQuit() {}

  @override
  Future<void> ensureStarted({required TerminalHostConfig config}) async {}

  @override
  Future<Map<String, Object?>?> probeRuntimeStatus() async {
    probeCalls += 1;
    return status;
  }

  @override
  Future<RuntimeHostShutdownResult> shutdownRuntime({
    bool force = false,
  }) async {
    return RuntimeHostShutdownResult(stopped: true, forced: force);
  }
}

final class _FakeSettingsRepository implements SettingsRepository {
  @override
  Future<AleraSettings> load() async => AleraSettings.defaults;

  @override
  Future<void> save(AleraSettings settings) async {}
}
