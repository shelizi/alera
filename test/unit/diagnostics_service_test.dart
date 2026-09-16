import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/diagnostics/infra/diagnostics_bundle_builder.dart';
import 'package:alera/src/features/diagnostics/infra/diagnostics_service.dart';
import 'package:alera/src/shared/infra/logging/app_logger.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:package_info_plus/package_info_plus.dart';

void main() {
  late Directory root;

  setUp(() async {
    await AppLogger.resetForTesting();
    root = await Directory.systemTemp.createTemp('alera-diagnostics-service-');
    await AppLogger.configure(directory: root);
  });

  tearDown(() async {
    await AppLogger.resetForTesting();
    if (root.existsSync()) {
      await root.delete(recursive: true);
    }
  });

  test('export forwards the selected destination and runtime facts', () async {
    final runtimeLogs = Directory('${root.path}${Platform.pathSeparator}runtime')
      ..createSync();
    final captured = <String, Object?>{};
    final builder = DiagnosticsBundleBuilder(
      nativeWriter:
          ({
            required outputPath,
            required metadataJson,
            appLogDirectory,
            runtimeLogDirectory,
          }) async {
            captured
              ..['outputPath'] = outputPath
              ..['metadataJson'] = metadataJson
              ..['appLogDirectory'] = appLogDirectory
              ..['runtimeLogDirectory'] = runtimeLogDirectory;
          },
    );
    final service = DiagnosticsService(
      builder: builder,
      packageInfo: () async => PackageInfo(
        appName: 'Alera',
        packageName: 'dev.leynier.alera',
        version: '1.2.3',
        buildNumber: '456',
      ),
      now: () => DateTime.utc(2026, 9, 16, 1, 2, 3),
    );
    final destination = '${root.path}${Platform.pathSeparator}export.zip';

    await service.writeBundleTo(
      outputPath: destination,
      runtime: RuntimeDiagnosticsInfo(
        version: '2.0.0',
        commit: 'abc123',
        protocolVersion: 7,
        logDirectory: runtimeLogs.path,
        capabilities: const <String>['hostDiagnosticsLogsV1'],
      ),
    );

    expect(captured['outputPath'], destination);
    expect(captured['appLogDirectory'], root.path);
    expect(captured['runtimeLogDirectory'], runtimeLogs.path);
    final metadata = jsonDecode(captured['metadataJson']! as String)
        as Map<String, Object?>;
    final app = metadata['app']! as Map<String, Object?>;
    final runtime = metadata['runtime']! as Map<String, Object?>;
    expect(app['version'], '1.2.3+456');
    expect(runtime['version'], '2.0.0');
    expect(runtime['commit'], 'abc123');
    expect(runtime['protocolVersion'], 7);
    expect(runtime['capabilities'], <Object?>['hostDiagnosticsLogsV1']);
  });
}
