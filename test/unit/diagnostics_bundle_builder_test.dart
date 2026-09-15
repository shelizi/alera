import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/diagnostics/domain/diagnostics_bundle_metadata.dart';
import 'package:alera/src/features/diagnostics/infra/diagnostics_bundle_builder.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late Directory root;
  late Directory appLogs;
  late Directory runtimeLogs;

  setUp(() {
    root = Directory.systemTemp.createTempSync('alera-bundle');
    appLogs = Directory('${root.path}/app-logs')..createSync();
    runtimeLogs = Directory('${root.path}/runtime-logs')..createSync();
  });

  tearDown(() {
    if (root.existsSync()) {
      root.deleteSync(recursive: true);
    }
  });

  DiagnosticsBundleMetadata metadata({
    String? runtimeVersion,
    List<String> capabilities = const <String>[],
  }) {
    return DiagnosticsBundleMetadata(
      appVersion: '0.34.0+63',
      flavor: 'dev',
      operatingSystem: 'linux',
      operatingSystemVersion: 'test-kernel',
      collectedAt: .utc(2026, 7, 28, 12),
      runtimeHostVersion: runtimeVersion,
      runtimeHostCommit: runtimeVersion == null ? null : 'abc1234',
      protocolVersion: runtimeVersion == null ? null : 4,
      runtimeCapabilities: capabilities,
    );
  }

  test(
    'forwards output, log directories, and metadata to the native writer',
    () async {
      String? capturedOutputPath;
      String? capturedMetadataJson;
      String? capturedAppLogDirectory;
      String? capturedRuntimeLogDirectory;
      final builder = DiagnosticsBundleBuilder(
        nativeWriter:
            ({
              required outputPath,
              required metadataJson,
              appLogDirectory,
              runtimeLogDirectory,
            }) async {
              capturedOutputPath = outputPath;
              capturedMetadataJson = metadataJson;
              capturedAppLogDirectory = appLogDirectory;
              capturedRuntimeLogDirectory = runtimeLogDirectory;
              return;
            },
      );
      final outputPath = '${root.path}/bundle.zip';

      await builder.writeToFile(
        outputPath: outputPath,
        metadata: metadata(
          runtimeVersion: '0.1.0',
          capabilities: <String>['hostDiagnosticsLogsV1'],
        ),
        appLogDirectory: appLogs,
        runtimeLogDirectory: runtimeLogs,
      );

      expect(capturedOutputPath, outputPath);
      expect(capturedAppLogDirectory, appLogs.path);
      expect(capturedRuntimeLogDirectory, runtimeLogs.path);
      final meta = jsonDecode(capturedMetadataJson!) as Map<String, Object?>;
      expect((meta['app']! as Map<String, Object?>)['version'], '0.34.0+63');
      expect((meta['app']! as Map<String, Object?>)['flavor'], 'dev');
      final runtime = meta['runtime']! as Map<String, Object?>;
      expect(runtime['reachable'], isTrue);
      expect(runtime['version'], '0.1.0');
      expect(runtime['protocolVersion'], 4);
      expect(runtime['capabilities'], contains('hostDiagnosticsLogsV1'));
    },
  );

  test('does not scan or reject missing log directories in Dart', () async {
    final missingApp = Directory('${root.path}/does-not-exist');
    final missingRuntime = Directory('${root.path}/also-missing');
    String? capturedAppLogDirectory;
    String? capturedRuntimeLogDirectory;
    final builder = DiagnosticsBundleBuilder(
      nativeWriter:
          ({
            required outputPath,
            required metadataJson,
            appLogDirectory,
            runtimeLogDirectory,
          }) async {
            capturedAppLogDirectory = appLogDirectory;
            capturedRuntimeLogDirectory = runtimeLogDirectory;
            return;
          },
    );

    await builder.writeToFile(
      outputPath: '${root.path}/bundle.zip',
      metadata: metadata(),
      appLogDirectory: missingApp,
      runtimeLogDirectory: missingRuntime,
    );

    expect(capturedAppLogDirectory, missingApp.path);
    expect(capturedRuntimeLogDirectory, missingRuntime.path);
  });

  test('suggested file name is filesystem safe', () {
    final name = DiagnosticsBundleBuilder.suggestedFileName(
      .utc(2026, 7, 28, 12, 30, 15),
    );

    expect(name, 'alera-diagnostics-20260728T123015.zip');
    expect(name, isNot(contains(':')));
  });
}
