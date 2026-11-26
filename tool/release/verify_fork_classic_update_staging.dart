import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/updater/domain/fork_classic_update_target.dart';
import 'package:desktop_updater/src/core/artifact_verifier.dart';
import 'package:desktop_updater/src/core/release_descriptor.dart';
import 'package:desktop_updater/src/core/update_client.dart';
import 'package:desktop_updater/src/io/file_update_transport.dart';
import 'package:desktop_updater/src/io/update_transport.dart';
import 'package:desktop_updater/src/version_info.dart';
import 'package:path/path.dart' as p;

import 'fork_classic_update_metadata.dart';

/// Validate the exact signed release archive using the same staging engine as
/// Classic, on its target OS. Never install or launch the application.
Future<void> main(List<String> args) async {
  if (args.length != 3) {
    throw const FormatException(
      'Usage: verify_fork_classic_update_staging.dart <target> <version+build> <dist>',
    );
  }
  final target = ForkClassicUpdateTarget.values.singleWhere(
    (value) => value.slug == args[0],
  );
  if (Platform.operatingSystem != target.platform) {
    throw StateError('Staging must be verified on ${target.platform}.');
  }
  final parts = args[1].split('+');
  if (parts.length != 2) {
    throw const FormatException('Expected version+build.');
  }
  final version = parts.first;
  final build = int.parse(parts.last);
  final directory = Directory(args[2]).absolute;
  final key = Platform.environment['ALERA_FORK_UPDATE_PUBLIC_KEY'] ?? '';
  await verifyForkClassicUpdateMetadata(
    target: target,
    version: version,
    buildNumber: build,
    directory: directory,
    publicKey: key,
  );
  final descriptor = ReleaseDescriptor.fromJson(
    jsonDecode(
      await File(p.join(directory.path, target.descriptorName)).readAsString(),
    ) as Map<String, dynamic>,
  );
  final scratch = await Directory.systemTemp.createTemp(
    'alera-fork-stage-smoke-',
  );
  try {
    final client = UpdateClient(
      appArchiveUrl: target.indexUrl,
      currentVersion: DesktopVersionInfo.fromParts(
        versionName: '0.0.0',
        buildNumber: '0',
      ),
      platform: target.platform,
      stagingParent: scratch,
      transport: _LocalSignedArtifact(
        descriptor.artifact.url,
        File(p.join(directory.path, target.artifactName(version))),
      ),
      verifier: ArtifactVerifier(
        policy: ArtifactVerificationPolicy.requireEd25519Signature(
          publicKeys: {ForkClassicUpdateTarget.keyId: key},
        ),
      ),
    );
    final staged = await client.downloadVerifyAndStage(descriptor: descriptor);
    final stagedPath = staged.stagingPath;
    if (!p.isWithin(scratch.path, stagedPath)) {
      throw StateError('Updater staged outside its isolated directory.');
    }
    if (target == ForkClassicUpdateTarget.macosArm64) {
      if (p.basename(stagedPath) != target.descriptorAppName ||
          await FileSystemEntity.type(stagedPath, followLinks: false) !=
              FileSystemEntityType.directory) {
        throw StateError('Staged path is not the expected real .app bundle.');
      }
      final plist = p.join(stagedPath, 'Contents', 'Info.plist');
      final identity = await Process.run('/usr/bin/plutil', [
        '-extract',
        'CFBundleIdentifier',
        'raw',
        '-o',
        '-',
        plist,
      ]);
      if (identity.exitCode != 0 ||
          identity.stdout.toString().trim() != 'dev.leynier.alera') {
        throw StateError('Staged macOS bundle identifier mismatch.');
      }
      final executable = await Process.run('/usr/bin/plutil', [
        '-extract',
        'CFBundleExecutable',
        'raw',
        '-o',
        '-',
        plist,
      ]);
      if (executable.exitCode != 0 ||
          executable.stdout.toString().trim() != 'Alera') {
        throw StateError('Staged macOS executable identity mismatch.');
      }
      await _requireFile(p.join(stagedPath, 'Contents', 'MacOS', 'Alera'));
      final executableCheck = await Process.run('/bin/test', [
        '-x',
        p.join(stagedPath, 'Contents', 'MacOS', 'Alera'),
      ]);
      if (executableCheck.exitCode != 0) {
        throw StateError('Staged macOS binary is not executable.');
      }
    } else {
      for (final name in [
        'Alera.exe',
        'flutter_windows.dll',
        'desktop_updater_plugin.dll',
        'resources/alera/alera.exe',
      ]) {
        await _requireFile(p.join(stagedPath, name));
      }
    }
    final manifestParent = target == ForkClassicUpdateTarget.macosArm64
        ? p.dirname(stagedPath)
        : stagedPath;
    await _requireFile(
      p.join(manifestParent, '.desktop_updater_release_manifest.json'),
    );
    stdout.writeln(
      'Classic ${target.slug} signed artifact staging smoke passed.',
    );
  } finally {
    await scratch.delete(recursive: true);
  }
}

Future<void> _requireFile(String path) async {
  if (await FileSystemEntity.type(path, followLinks: false) !=
          FileSystemEntityType.file ||
      await File(path).length() == 0) {
    throw StateError('Missing or invalid staged file: $path');
  }
}

/// Feed the exact local CI artifact to the ordinary updater download engine.
/// No metadata rewriting and no network dependency in the packaging smoke.
class _LocalSignedArtifact implements UpdateTransport {
  const _LocalSignedArtifact(this.expectedUrl, this.artifact);

  final Uri expectedUrl;
  final File artifact;

  @override
  Future<void> download(
    Uri source,
    File destination, {
    void Function(int receivedBytes, int? totalBytes)? onProgress,
    Duration? timeout,
  }) {
    if (source != expectedUrl) {
      throw StateError('Unexpected updater artifact URL.');
    }
    return const FileUpdateTransport().download(
      artifact.uri,
      destination,
      onProgress: onProgress,
      timeout: timeout,
    );
  }
}
