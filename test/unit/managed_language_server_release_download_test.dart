import 'dart:ffi' show Abi;
import 'dart:io';

import 'package:alera/src/features/language_intelligence/application/managed_language_server_installer.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/infra/managed_language_server_catalog.dart';
import 'package:alera/src/features/language_intelligence/infra/managed_language_server_installer.dart';
import 'package:alera/src/features/language_intelligence/infra/managed_language_server_release_download.dart';
import 'package:archive/archive.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  late Directory temp;
  final provider = LanguageProviderDescriptor(
    id: 'python.fake-release',
    kind: LanguageProviderKind.semanticServer,
    languages: <LanguageId>{LanguageId('python')},
    capabilities: const <LanguageCapability>{LanguageCapability.definition},
    processScope: LanguageProviderProcessScope.workspace,
    launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
    executableResolutionPolicy:
        LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
    executableCandidates: const <String>['fake'],
  );
  final binary = List<int>.generate(4096, (index) => index % 251);

  setUp(() async {
    temp = await Directory.systemTemp.createTemp('alera-release-lsp-test-');
  });

  tearDown(() async {
    if (await temp.exists()) await temp.delete(recursive: true);
  });

  ManagedLanguageServerInstaller installer({
    required List<int> archive,
    required String url,
    required String member,
    required bool isWindows,
    String? sha256Override,
    ManagedLanguageServerPlatform? platform =
        ManagedLanguageServerPlatform.linuxX64,
    _FakeDownload? download,
  }) {
    final recipe = ManagedLanguageServerRecipe(
      providerId: provider.id,
      version: '1.0.0',
      kind: ManagedLanguageServerInstallKind.githubRelease,
      executableName: 'fake',
      source: 'https://example.invalid/releases/1.0.0',
      integrityPolicy: 'Pinned SHA-256.',
      releaseAssets:
          <ManagedLanguageServerPlatform, ManagedLanguageServerReleaseAsset>{
            ManagedLanguageServerPlatform.linuxX64:
                ManagedLanguageServerReleaseAsset(
                  url: url,
                  sha256: sha256Override ?? sha256.convert(archive).toString(),
                  archiveMember: member,
                ),
          },
    );
    return ManagedLanguageServerInstaller(
      environmentReader: () => const <String, String>{},
      isWindows: isWindows,
      executableExists: (path) => File(path).existsSync(),
      supportDirectory: () async => temp,
      recipes: <String, ManagedLanguageServerRecipe>{provider.id: recipe},
      download: download ?? _FakeDownload(archive),
      platform: platform,
    );
  }

  test(
    'installs the pinned member of a zip and reuses the verified copy',
    () async {
      final archive = _zip(<String, List<int>>{
        'fake.exe': binary,
        'README.md': 'docs'.codeUnits,
      });
      final download = _FakeDownload(archive);
      final subject = installer(
        archive: archive,
        url: 'https://example.invalid/fake-windows.zip',
        member: 'fake.exe',
        isWindows: true,
        download: download,
      );

      final result = await subject.ensureInstalled(provider);

      final installed = result as ManagedLanguageServerInstalled;
      expect(p.basename(installed.executable), 'fake.exe');
      expect(await File(installed.executable).readAsBytes(), binary);
      // Only the pinned member is written; nothing else from the archive.
      expect(
        File(p.join(p.dirname(installed.executable), 'README.md')).existsSync(),
        isFalse,
      );
      expect(
        File(p.join(p.dirname(installed.executable), '.release-archive.part'))
            .existsSync(),
        isFalse,
      );

      await subject.ensureInstalled(provider);
      expect(download.calls, 1);
    },
  );

  test('extracts a nested member from a tar.gz archive', () async {
    final archive = _tarGz(<String, List<int>>{
      'fake-x86_64-linux/fake': binary,
    });
    final subject = installer(
      archive: archive,
      url: 'https://example.invalid/fake-linux.tar.gz',
      member: 'fake-x86_64-linux/fake',
      isWindows: false,
    );

    final installed = await subject.ensureInstalled(
      provider,
    ) as ManagedLanguageServerInstalled;

    expect(p.basename(installed.executable), 'fake');
    expect(await File(installed.executable).readAsBytes(), binary);
    if (!Platform.isWindows) {
      expect(
        (await File(installed.executable).stat()).mode & 0x40,
        isNonZero,
        reason: 'owner execute bit',
      );
    }
  });

  test('rejects an archive whose checksum does not match the pin', () async {
    final archive = _zip(<String, List<int>>{'fake.exe': binary});
    final subject = installer(
      archive: archive,
      url: 'https://example.invalid/fake-windows.zip',
      member: 'fake.exe',
      isWindows: true,
      sha256Override: '0' * 64,
    );

    await expectLater(
      subject.ensureInstalled(provider),
      throwsA(
        isA<ManagedLanguageServerInstallException>().having(
          (error) => error.message,
          'message',
          contains('Checksum mismatch'),
        ),
      ),
    );
    final versionDirectory = Directory(
      p.join(temp.path, 'language-servers', provider.id, '1.0.0'),
    );
    expect(versionDirectory.existsSync(), isFalse);
  });

  test('rejects an archive without the pinned member', () async {
    final archive = _zip(<String, List<int>>{'other.exe': binary});
    final subject = installer(
      archive: archive,
      url: 'https://example.invalid/fake-windows.zip',
      member: 'fake.exe',
      isWindows: true,
    );

    await expectLater(
      subject.ensureInstalled(provider),
      throwsA(isA<ManagedLanguageServerInstallException>()),
    );
  });

  test('reports a platform without a pinned build as unavailable', () async {
    final download = _FakeDownload(const <int>[]);
    final subject = installer(
      archive: const <int>[],
      url: 'https://example.invalid/fake.zip',
      member: 'fake.exe',
      isWindows: true,
      platform: ManagedLanguageServerPlatform.macosArm64,
      download: download,
    );

    final result = await subject.ensureInstalled(provider);

    expect(result, isA<ManagedLanguageServerUnavailable>());
    expect(
      (result as ManagedLanguageServerUnavailable).reason,
      contains('no fake 1.0.0 build for this platform'),
    );
    expect(download.calls, 0);
  });

  test('maps every desktop ABI Alera ships for', () {
    expect(
      managedLanguageServerPlatformFor(Abi.windowsX64),
      ManagedLanguageServerPlatform.windowsX64,
    );
    expect(
      managedLanguageServerPlatformFor(Abi.macosArm64),
      ManagedLanguageServerPlatform.macosArm64,
    );
    expect(
      managedLanguageServerPlatformFor(Abi.linuxArm64),
      ManagedLanguageServerPlatform.linuxArm64,
    );
    expect(managedLanguageServerPlatformFor(Abi.androidArm64), isNull);
  });

  test('release recipes pin every desktop platform by URL and SHA-256', () {
    for (final id in const <String>['python.pyrefly', 'python.ty']) {
      final recipe = managedLanguageServerRecipes[id]!;
      expect(recipe.kind, ManagedLanguageServerInstallKind.githubRelease);
      expect(
        recipe.releaseAssets.keys,
        unorderedEquals(ManagedLanguageServerPlatform.values),
      );
      for (final asset in recipe.releaseAssets.values) {
        expect(asset.url, startsWith('https://github.com/'));
        expect(asset.url, contains('/releases/download/${recipe.version}/'));
        expect(asset.sha256, matches(RegExp(r'^[0-9a-f]{64}$')));
        expect(
          p.posix.basename(asset.archiveMember),
          anyOf(recipe.executableName, '${recipe.executableName}.exe'),
        );
      }
    }
  });
}

List<int> _zip(Map<String, List<int>> files) {
  final archive = Archive();
  for (final entry in files.entries) {
    archive.addFile(ArchiveFile.bytes(entry.key, entry.value));
  }
  return ZipEncoder().encode(archive);
}

List<int> _tarGz(Map<String, List<int>> files) {
  final archive = Archive();
  for (final entry in files.entries) {
    archive.addFile(ArchiveFile.bytes(entry.key, entry.value)..mode = 0x1ED);
  }
  return GZipEncoder().encode(TarEncoder().encode(archive));
}

final class _FakeDownload implements ManagedLanguageServerDownloadPort {
  _FakeDownload(this.bytes);

  final List<int> bytes;
  int calls = 0;

  @override
  Future<void> download(
    Uri url,
    File destination, {
    required int maxBytes,
  }) async {
    calls += 1;
    await destination.writeAsBytes(bytes, flush: true);
  }
}
