import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/agent_status/infra/claude_runtime_home_service.dart';
import 'package:alera/src/features/agent_status/infra/codex_runtime_home_service.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

// Captured with Flutter 3.44.8 / Dart 3.12.2 for a file containing
// "plugin contents" with mtime 2026-01-02T03:04:05.123456Z. The services use
// different JSON key ordering, so their persisted hashes are distinct.
const _legacyFingerprints = <String, String>{
  'codex':
      'sha256:b105eea1dcbe75b3f192a566236a62ae9cad43cbea0f2b74231bea442e66dd6c',
  'claude':
      'sha256:a9fa862cdec6b7d664d8dee2faf80f3828f66ecfcbddcb67848eca9d6bfd4f59',
};

void main() {
  for (final agent in _legacyFingerprints.keys) {
    group('$agent resource fingerprint compatibility', () {
      late Directory root;
      late Directory home;
      late Directory support;
      late File source;
      final modified = DateTime.utc(2026, 1, 2, 3, 4, 5, 123, 456);

      setUp(() {
        root = Directory.systemTemp.createTempSync('alera-resource-upgrade-');
        home = Directory(p.join(root.path, 'home'))..createSync();
        support = Directory(p.join(root.path, 'support'))..createSync();
        source = File(p.join(home.path, '.$agent', 'plugins'))
          ..createSync(recursive: true)
          ..writeAsStringSync('plugin contents')
          ..setLastModifiedSync(modified);
      });

      tearDown(() => root.deleteSync(recursive: true));

      Future<String> prepare() => _prepareRuntime(agent, home, support);

      test('retains a copied resource with a pre-upgrade marker', () async {
        final runtimeHome = await prepare();
        final target = File(p.join(runtimeHome, 'plugins'))
          ..writeAsStringSync('local runtime customization');
        final marker = File(p.join(runtimeHome, '.alera-copied-plugins.json'))
          ..writeAsStringSync(
            jsonEncode(<String, String>{
              'sourcePath': source.path,
              'sourceFingerprint': _legacyFingerprints[agent]!,
            }),
          );
        final markerBefore = marker.readAsStringSync();

        await prepare();

        expect(target.readAsStringSync(), 'local runtime customization');
        expect(marker.readAsStringSync(), markerBefore);
        expect(source.readAsStringSync(), 'plugin contents');
      });

      test('still refreshes a changed source with the same length', () async {
        final runtimeHome = await prepare();
        final target = File(p.join(runtimeHome, 'plugins'));
        final marker = File(p.join(runtimeHome, '.alera-copied-plugins.json'));
        final markerBefore = marker.readAsStringSync();
        source
          ..writeAsStringSync('changed content')
          ..setLastModifiedSync(modified.add(const Duration(milliseconds: 1)));

        await prepare();

        expect(target.readAsStringSync(), 'changed content');
        expect(marker.readAsStringSync(), isNot(markerBefore));
      });

      test('never replaces a resource without an ownership marker', () async {
        final runtimeHome = await prepare();
        final target = File(p.join(runtimeHome, 'plugins'))
          ..writeAsStringSync('unmanaged resource');
        final marker = File(p.join(runtimeHome, '.alera-copied-plugins.json'))
          ..deleteSync();
        source.writeAsStringSync('source changed');

        await prepare();

        expect(target.readAsStringSync(), 'unmanaged resource');
        expect(marker.existsSync(), isFalse);
      });

      test('uses an injected asynchronous resource fingerprinter', () async {
        final fingerprintedPaths = <String>[];
        const injectedFingerprint = 'sha256:injected-native-fingerprint';

        final runtimeHome = await _prepareRuntime(
          agent,
          home,
          support,
          resourceFingerprinter: (sourcePath) async {
            fingerprintedPaths.add(sourcePath);
            return injectedFingerprint;
          },
        );

        final marker = File(p.join(runtimeHome, '.alera-copied-plugins.json'));
        final decoded = jsonDecode(marker.readAsStringSync()) as Map;
        expect(decoded['sourceFingerprint'], injectedFingerprint);
        expect(fingerprintedPaths, contains(source.path));
      });

      test('uses an injected asynchronous resource copier', () async {
        final copiedSources = <String>[];

        final runtimeHome = await _prepareRuntime(
          agent,
          home,
          support,
          resourceCopier: ({required sourcePath, required targetPath}) async {
            copiedSources.add(sourcePath);
            File(sourcePath).copySync(targetPath);
          },
        );

        expect(copiedSources, contains(source.path));
        expect(
          File(p.join(runtimeHome, 'plugins')).readAsStringSync(),
          'plugin contents',
        );
      });

      test(
        'reuses a precomputed fingerprint during resource reconcile',
        () async {
          final runtimeHome = await prepare();
          const refreshedFingerprint = 'sha256:refreshed';
          var fingerprintCalls = 0;
          String? reconcilerFingerprint;
          source
            ..writeAsStringSync('changed content')
            ..setLastModifiedSync(
              modified.add(const Duration(milliseconds: 1)),
            );

          await _prepareRuntime(
            agent,
            home,
            support,
            resourceFingerprinter: (sourcePath) async {
              fingerprintCalls += 1;
              return refreshedFingerprint;
            },
            resourceReconciler:
                ({
                  required sourcePath,
                  required targetPath,
                  knownFingerprint,
                }) async {
                  reconcilerFingerprint = knownFingerprint;
                  final type = FileSystemEntity.typeSync(
                    targetPath,
                    followLinks: false,
                  );
                  if (type == FileSystemEntityType.directory) {
                    Directory(targetPath).deleteSync(recursive: true);
                  } else if (type == FileSystemEntityType.link) {
                    Link(targetPath).deleteSync();
                  } else if (type != FileSystemEntityType.notFound) {
                    File(targetPath).deleteSync();
                  }
                  File(sourcePath).copySync(targetPath);
                  return knownFingerprint ?? refreshedFingerprint;
                },
          );

          final marker = File(
            p.join(runtimeHome, '.alera-copied-plugins.json'),
          );
          final decoded = jsonDecode(marker.readAsStringSync()) as Map;
          expect(fingerprintCalls, 1);
          expect(reconcilerFingerprint, refreshedFingerprint);
          expect(decoded['sourceFingerprint'], refreshedFingerprint);
        },
      );

      test('uses an injected asynchronous resource deleter', () async {
        final runtimeHome = await prepare();
        final targetPath = p.join(runtimeHome, 'plugins');
        final deletedPaths = <String>[];
        source.deleteSync();

        await _prepareRuntime(
          agent,
          home,
          support,
          resourceDeleter: (path) async {
            deletedPaths.add(path);
            final type = FileSystemEntity.typeSync(path, followLinks: false);
            if (type == FileSystemEntityType.directory) {
              Directory(path).deleteSync(recursive: true);
            } else if (type == FileSystemEntityType.link) {
              Link(path).deleteSync();
            } else if (type != FileSystemEntityType.notFound) {
              File(path).deleteSync();
            }
          },
        );

        expect(deletedPaths, contains(targetPath));
        expect(
          FileSystemEntity.typeSync(targetPath),
          FileSystemEntityType.notFound,
        );
      });
    });
  }
}

Future<String> _prepareRuntime(
  String agent,
  Directory home,
  Directory support, {
  Future<String> Function(String sourcePath)? resourceFingerprinter,
  Future<void> Function({
    required String sourcePath,
    required String targetPath,
  })?
  resourceCopier,
  Future<void> Function(String path)? resourceDeleter,
  Future<String> Function({
    required String sourcePath,
    required String targetPath,
    String? knownFingerprint,
  })?
  resourceReconciler,
}) async {
  void failLinks({required String sourcePath, required String targetPath}) {
    throw const FileSystemException('symlinks disabled');
  }

  if (agent == 'codex') {
    final runtimeHome = Directory(p.join(support.path, 'codex-runtime'))
      ..createSync(recursive: true);
    final service = CodexRuntimeHomeService(
      homeDirectory: home.path,
      applicationSupportDirectory: () async => support,
      platform: .posix,
      environment: <String, String>{'HOME': home.path},
      resourceLinkCreator: failLinks,
      resourceFingerprinter: resourceFingerprinter,
      resourceCopier: resourceCopier,
      resourceDeleter: resourceDeleter,
      resourceReconciler: resourceReconciler,
    );
    await service.install(runtimeHome: runtimeHome);
    return runtimeHome.path;
  }
  final preparation = await ClaudeRuntimeHomeService(
    homeDirectory: home.path,
    applicationSupportDirectory: () async => support,
    platform: .posix,
    environment: <String, String>{'HOME': home.path},
    resourceLinkCreator: failLinks,
    resourceFingerprinter: resourceFingerprinter,
    resourceCopier: resourceCopier,
    resourceDeleter: resourceDeleter,
    resourceReconciler: resourceReconciler,
    syncMacOSKeychainCredentials: false,
  ).prepareForTerminalLaunch();
  return preparation.runtimeHomePath;
}
