import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:alera/src/features/updater/domain/alera_update.dart';
import 'package:alera/src/features/updater/domain/fork_classic_update_target.dart';
import 'package:alera/src/features/updater/infra/desktop_updater_backend.dart';
import 'package:archive/archive.dart';
import 'package:cryptography/cryptography.dart';
import 'package:desktop_updater/desktop_updater.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:path/path.dart' as p;

import '../../tool/release/fork_classic_update_metadata.dart';

void main() {
  const version = '1.2.3';
  const build = 42;
  late Directory directory;
  late String publicKey;
  late String privateSeed;
  late SimpleKeyPair keyPair;

  setUp(() async {
    directory = await Directory.systemTemp.createTemp('classic-fork-update-');
    final seed = List<int>.filled(32, 7);
    keyPair = await Ed25519().newKeyPairFromSeed(seed);
    publicKey = base64Encode((await keyPair.extractPublicKey()).bytes);
    privateSeed = base64Encode(seed);
  });
  tearDown(() => directory.delete(recursive: true));

  test('matches exactly the supported runtime ABIs', () {
    expect(
      ForkClassicUpdateTarget.forAbi(Abi.windowsX64),
      ForkClassicUpdateTarget.windowsX64,
    );
    expect(
      ForkClassicUpdateTarget.forAbi(Abi.macosArm64),
      ForkClassicUpdateTarget.macosArm64,
    );
    for (final abi in [
      Abi.windowsArm64,
      Abi.macosX64,
      Abi.linuxX64,
      Abi.androidArm64,
    ]) {
      expect(ForkClassicUpdateTarget.forAbi(abi), isNull);
    }
  });

  Future<void> writeZip(
    ForkClassicUpdateTarget target, {
    String? macRoot,
  }) async {
    final root = macRoot ?? target.descriptorAppName;
    final entries = target == ForkClassicUpdateTarget.macosArm64
        ? ['$root/Contents/Info.plist', '$root/Contents/MacOS/Alera']
        : [
            'Alera.exe',
            'flutter_windows.dll',
            'desktop_updater_plugin.dll',
            'resources/alera/alera.exe',
          ];
    final archive = Archive();
    for (final entry in entries) {
      archive.addFile(ArchiveFile.bytes(entry, [1, 2, 3, 4]));
    }
    await File(p.join(directory.path, target.artifactName(version)))
        .writeAsBytes(ZipEncoder().encode(archive));
  }

  Future<void> generate(ForkClassicUpdateTarget target) async {
    await writeZip(target);
    await generateForkClassicUpdateMetadata(
      target: target,
      version: version,
      buildNumber: build,
      directory: directory,
      privateSeed: privateSeed,
      publicKey: publicKey,
    );
    await verifyForkClassicUpdateMetadata(
      target: target,
      version: version,
      buildNumber: build,
      directory: directory,
      publicKey: publicKey,
    );
  }

  File descriptorFile(ForkClassicUpdateTarget target) =>
      File(p.join(directory.path, target.descriptorName));

  Future<DesktopUpdaterReleaseCandidate?> check(
    ForkClassicUpdateTarget target, {
    String currentVersion = '1.0.0',
    String currentBuild = '1',
    bool signed = true,
    Uri? archive,
  }) async {
    final client = MockClient((request) async {
      if (request.url == target.indexUrl) {
        return http.Response(
          await File(p.join(directory.path, target.indexName)).readAsString(),
          200,
        );
      }
      if (request.url == target.descriptorUrl(version)) {
        return http.Response(await descriptorFile(target).readAsString(), 200);
      }
      fail('Unexpected network request: ${request.url}');
    });
    final backend = DesktopUpdaterBackend(client: client, forkTarget: target);
    addTearDown(client.close);
    addTearDown(backend.dispose);
    return backend.checkForUpdate(
      archiveUrl: archive ?? target.indexUrl,
      channel: 'stable',
      currentVersion: currentVersion,
      currentBuildNumber: currentBuild,
      platform: target.platform,
      requireSignature: signed,
      publicKeyId: ForkClassicUpdateTarget.keyId,
      publicKeyBase64: publicKey,
    );
  }

  for (final target in ForkClassicUpdateTarget.values) {
    test(
      '${target.slug} signed CI metadata is accepted by the real backend',
      () async {
        await generate(target);
        final candidate = await check(target);
        expect(candidate?.version, version);
        expect(candidate?.buildNumber, build);
        expect(candidate?.platform, target.platform);
        expect(candidate?.artifactUrl, target.artifactUrl(version));
        expect(
          candidate?.artifactLength,
          await File(p.join(directory.path, target.artifactName(version)))
              .length(),
        );
        final descriptor = jsonDecode(
          await descriptorFile(target).readAsString(),
        ) as Map<String, dynamic>;
        expect(
          descriptor['appName'],
          target == ForkClassicUpdateTarget.macosArm64 ? 'Alera.app' : 'Alera',
        );
        expect(
          await check(target, currentVersion: version, currentBuild: '42'),
          isNull,
        );
        expect(await check(target, currentVersion: '2.0.0'), isNull);
        expect(
          await check(target, currentVersion: version, currentBuild: '1'),
          isNull,
        );
        expect(
          (await check(
            target,
            currentVersion: '1.2.2',
            currentBuild: '9999',
          ))?.version,
          version,
        );
        expect(await check(target, currentVersion: '1.10.0'), isNull);
      },
    );

    test(
      '${target.slug} refuses unsigned or upstream update discovery',
      () async {
        await generate(target);
        await expectLater(check(target, signed: false), throwsFormatException);
        await expectLater(
          check(target, archive: AleraUpdateConfig.defaultArchiveUrl),
          throwsFormatException,
        );
      },
    );

    test('${target.slug} rejects a tampered artifact at publication', () async {
      await generate(target);
      await File(p.join(directory.path, target.artifactName(version)))
          .writeAsBytes([9, 8, 7]);
      await expectLater(
        verifyForkClassicUpdateMetadata(
          target: target,
          version: version,
          buildNumber: build,
          directory: directory,
          publicKey: publicKey,
        ),
        throwsFormatException,
      );
    });

    test(
      '${target.slug} rejects unsigned and validly signed wrong artifacts',
      () async {
        await generate(target);
        final file = descriptorFile(target);
        final json =
            jsonDecode(await file.readAsString()) as Map<String, dynamic>;
        final unsigned = Map<String, dynamic>.from(json)..remove('signature');
        await file.writeAsString(jsonEncode(unsigned));
        await expectLater(check(target), throwsFormatException);

        final other = ForkClassicUpdateTarget.values.firstWhere(
          (value) => value != target,
        );
        (json['artifact'] as Map<String, dynamic>)['url'] = other
            .artifactUrl(version)
            .toString();
        (json['signature'] as Map<String, dynamic>)['value'] = '';
        final descriptor = ReleaseDescriptor.fromJson(json);
        final signature = await Ed25519().sign(
          descriptor.canonicalSignatureBytes(),
          keyPair: keyPair,
        );
        (json['signature'] as Map<String, dynamic>)['value'] = base64Encode(
          signature.bytes,
        );
        await file.writeAsString(jsonEncode(json));
        await expectLater(check(target), throwsFormatException);
      },
    );

    test(
      '${target.slug} rejects an index pointing outside its release asset',
      () async {
        await generate(target);
        final file = File(p.join(directory.path, target.indexName));
        final json =
            jsonDecode(await file.readAsString()) as Map<String, dynamic>;
        ((json['items'] as List).single as Map)['release'] = 'https://github.com/leynier/alera/releases/download/v1.2.3/release.json';
        await file.writeAsString(jsonEncode(json));
        await expectLater(check(target), throwsFormatException);
      },
    );
  }

  test(
    'macOS rejects a signed descriptor with the legacy non-bundle name',
    () async {
      const target = ForkClassicUpdateTarget.macosArm64;
      await generate(target);
      final file = descriptorFile(target);
      final json =
          jsonDecode(await file.readAsString()) as Map<String, dynamic>;
      json['appName'] = 'Alera';
      (json['signature'] as Map<String, dynamic>)['value'] = '';
      final descriptor = ReleaseDescriptor.fromJson(json);
      final signed = await Ed25519().sign(
        descriptor.canonicalSignatureBytes(),
        keyPair: keyPair,
      );
      (json['signature'] as Map<String, dynamic>)['value'] = base64Encode(
        signed.bytes,
      );
      await file.writeAsString(jsonEncode(json));
      await expectLater(check(target), throwsFormatException);
    },
  );

  test('macOS cannot sign a ZIP with a mismatched bundle root', () async {
    const target = ForkClassicUpdateTarget.macosArm64;
    await writeZip(target, macRoot: 'Alera');
    await expectLater(
      generateForkClassicUpdateMetadata(
        target: target,
        version: version,
        buildNumber: build,
        directory: directory,
        privateSeed: privateSeed,
        publicKey: publicKey,
      ),
      throwsFormatException,
    );
    expect(await descriptorFile(target).exists(), isFalse);
  });

  test(
    'discovery requires the fork index, stable channel and pinned public key',
    () {
      const target = ForkClassicUpdateTarget.windowsX64;
      final config = AleraUpdateConfig(
        archiveUrl: target.indexUrl,
        releasePageUrl: Uri.parse('https://github.com/shelizi/alera/releases'),
        channel: .stable,
        autoInstallEnabled: true,
        signedRelease: true,
        manifestPublicKey: publicKey,
        manifestPublicKeyId: ForkClassicUpdateTarget.keyId,
      );
      expect(target.allowsDiscovery(config), isTrue);
      expect(
        target.allowsDiscovery(config.copyWith(signedRelease: false)),
        isFalse,
      );
      expect(
        target.allowsDiscovery(config.copyWith(manifestPublicKey: 'bad')),
        isFalse,
      );
      expect(
        target.allowsDiscovery(
          config.copyWith(archiveUrl: AleraUpdateConfig.defaultArchiveUrl),
        ),
        isFalse,
      );
      expect(
        ForkClassicUpdateTarget.macosArm64.allowsDiscovery(config),
        isFalse,
      );
      expect(() => target.artifactName('1.2.3-rc.1'), throwsFormatException);
    },
  );
}
