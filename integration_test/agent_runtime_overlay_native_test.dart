import 'dart:io';

import 'package:alera/src/rust/api/agent_runtime_overlay.dart' as native;
import 'package:alera/src/rust/frb_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:integration_test/integration_test.dart';
import 'package:path/path.dart' as p;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
    final libraryPath = Platform.environment['ALERA_NATIVE_LIBRARY_PATH'];
    await RustLib.init(
      externalLibrary: libraryPath == null
          ? null
          : ExternalLibrary.open(libraryPath),
    );
  });

  test('agent runtime overlay reconciles through the native bridge', () async {
    final root = await Directory.systemTemp.createTemp('alera-overlay-native-');
    addTearDown(() async {
      if (await root.exists()) {
        await root.delete(recursive: true);
      }
    });

    final source = Directory(p.join(root.path, 'source'))
      ..createSync(recursive: true);
    File(p.join(source.path, 'settings.json'))
        .writeAsStringSync('user-settings');
    final sourcePlugins = Directory(p.join(source.path, 'plugins'))
      ..createSync();
    File(p.join(sourcePlugins.path, 'user.js'))
        .writeAsStringSync('user-plugin');

    final overlayRoot = p.join(root.path, 'overlays');
    final overlayPath = p.join(overlayRoot, 'session');
    final result = await native.prepareAgentRuntimeOverlay(
      request: native.AgentRuntimeOverlayRequest(
        overlayRoot: overlayRoot,
        overlayPath: overlayPath,
        mirrorPath: null,
        sourcePath: source.path,
        managedSubdirectory: 'plugins',
        managedFileNames: const <String>['alera.js'],
        managedFiles: <native.AgentRuntimeOverlayManagedFile>[
          native.AgentRuntimeOverlayManagedFile(
            path: p.join(overlayPath, 'plugins', 'alera.js'),
            allowedRoot: overlayPath,
            content: 'managed-plugin',
            writeMode: native.AgentRuntimeOverlayWriteMode.replace,
            executable: false,
          ),
        ],
      ),
    );

    expect(result.sourceExists, isTrue);
    expect(
      File(p.join(overlayPath, 'settings.json')).readAsStringSync(),
      'user-settings',
    );
    expect(
      File(p.join(overlayPath, 'plugins', 'user.js')).readAsStringSync(),
      'user-plugin',
    );
    expect(
      File(p.join(overlayPath, 'plugins', 'alera.js')).readAsStringSync(),
      'managed-plugin',
    );

    final cleanup = await native.clearAgentRuntimeOverlays(
      targets: <native.AgentRuntimeOverlayCleanupTarget>[
        native.AgentRuntimeOverlayCleanupTarget(
          overlayRoot: overlayRoot,
          overlayPath: overlayPath,
        ),
      ],
    );

    expect(cleanup.removedCount, greaterThan(BigInt.zero));
    expect(Directory(overlayPath).existsSync(), isFalse);
    expect(
      File(p.join(source.path, 'settings.json')).readAsStringSync(),
      'user-settings',
    );
  });

  test('native bridge projects cleanup warnings to Dart', () async {
    final root = await Directory.systemTemp.createTemp(
      'alera-overlay-warning-',
    );
    addTearDown(() async {
      if (await root.exists()) {
        await root.delete(recursive: true);
      }
    });

    final overlayRoot = p.join(root.path, 'overlays');
    final outsidePath = p.join(root.path, 'outside');
    final result = await native.clearAgentRuntimeOverlays(
      targets: <native.AgentRuntimeOverlayCleanupTarget>[
        native.AgentRuntimeOverlayCleanupTarget(
          overlayRoot: overlayRoot,
          overlayPath: outsidePath,
        ),
      ],
    );

    expect(result.removedCount, BigInt.zero);
    expect(result.warnings, isNotEmpty);
    expect(result.warnings.single, contains('strictly contained'));
  });

  test('native bridge projects preparation errors to Dart', () async {
    final root = await Directory.systemTemp.createTemp('alera-overlay-error-');
    addTearDown(() async {
      if (await root.exists()) {
        await root.delete(recursive: true);
      }
    });

    final overlayRoot = p.join(root.path, 'overlays');
    final outsidePath = p.join(root.path, 'outside');
    final future = native.prepareAgentRuntimeOverlay(
      request: native.AgentRuntimeOverlayRequest(
        overlayRoot: overlayRoot,
        overlayPath: outsidePath,
        mirrorPath: null,
        sourcePath: null,
        managedSubdirectory: null,
        managedFileNames: const <String>[],
        managedFiles: const <native.AgentRuntimeOverlayManagedFile>[],
      ),
    );

    await expectLater(future, throwsA(anything));
  });
}
