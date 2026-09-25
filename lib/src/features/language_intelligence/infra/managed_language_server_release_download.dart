import 'dart:async';
import 'dart:ffi' show Abi;
import 'dart:io';
import 'dart:isolate';

import 'package:archive/archive.dart';
import 'package:crypto/crypto.dart';
import 'package:http/http.dart' as http;
import 'package:path/path.dart' as p;

import '../../../shared/infra/files/posix_file_mode.dart';
import 'managed_language_server_catalog.dart';
import 'managed_language_server_installer.dart';

/// Release archives for language servers are tens of megabytes; anything far
/// larger means the pinned URL no longer serves what the catalog describes.
const int managedReleaseArchiveMaxBytes = 256 * 1024 * 1024;

const int _executableFileMode = 0x1ED; // 0755

ManagedLanguageServerPlatform? managedLanguageServerPlatformFor(Abi abi) =>
    switch (abi) {
      Abi.windowsX64 => ManagedLanguageServerPlatform.windowsX64,
      Abi.windowsArm64 => ManagedLanguageServerPlatform.windowsArm64,
      Abi.macosX64 => ManagedLanguageServerPlatform.macosX64,
      Abi.macosArm64 => ManagedLanguageServerPlatform.macosArm64,
      Abi.linuxX64 => ManagedLanguageServerPlatform.linuxX64,
      Abi.linuxArm64 => ManagedLanguageServerPlatform.linuxArm64,
      _ => null,
    };

/// Network boundary for managed release downloads.
abstract interface class ManagedLanguageServerDownloadPort {
  Future<void> download(Uri url, File destination, {required int maxBytes});
}

final class HttpManagedLanguageServerDownload
    implements ManagedLanguageServerDownloadPort {
  HttpManagedLanguageServerDownload({http.Client Function()? clientFactory})
    : _clientFactory = clientFactory ?? http.Client.new;

  final http.Client Function() _clientFactory;

  @override
  Future<void> download(
    Uri url,
    File destination, {
    required int maxBytes,
  }) async {
    final client = _clientFactory();
    try {
      final response = await client.send(http.Request('GET', url));
      if (response.statusCode != HttpStatus.ok) {
        throw ManagedLanguageServerInstallException(
          'Download of $url failed with HTTP ${response.statusCode}.',
        );
      }
      final length = response.contentLength;
      if (length != null && length > maxBytes) {
        throw ManagedLanguageServerInstallException(
          'Download of $url is $length bytes, over the $maxBytes byte limit.',
        );
      }
      final sink = destination.openWrite();
      var received = 0;
      try {
        await for (final chunk in response.stream) {
          received += chunk.length;
          if (received > maxBytes) {
            throw ManagedLanguageServerInstallException(
              'Download of $url exceeded the $maxBytes byte limit.',
            );
          }
          sink.add(chunk);
        }
      } finally {
        await sink.close();
      }
    } finally {
      client.close();
    }
  }
}

/// Downloads [asset] into [installDirectory], verifies the archive against
/// the pinned checksum and extracts only the pinned executable member to
/// [executableFileName]. Returns the executable path.
///
/// Only the named member is written, to a path chosen here, so an archive
/// entry cannot escape the install directory. Hashing and decompression run
/// in a background isolate because the archives are tens of megabytes.
Future<String> installManagedReleaseAsset({
  required ManagedLanguageServerReleaseAsset asset,
  required Directory installDirectory,
  required String executableFileName,
  required ManagedLanguageServerDownloadPort download,
  required bool setExecutableMode,
}) async {
  final archivePath = p.join(installDirectory.path, '.release-archive.part');
  final archiveFile = File(archivePath);
  try {
    await download.download(
      Uri.parse(asset.url),
      archiveFile,
      maxBytes: managedReleaseArchiveMaxBytes,
    );
    final digest = await Isolate.run(() => _sha256OfFile(archivePath));
    if (digest != asset.sha256) {
      throw ManagedLanguageServerInstallException(
        'Checksum mismatch for ${asset.url}: expected ${asset.sha256}, '
        'got $digest.',
      );
    }
    final executablePath = p.join(installDirectory.path, executableFileName);
    final isZip = asset.url.toLowerCase().endsWith('.zip');
    final member = asset.archiveMember;
    final extracted = await Isolate.run(
      () => _extractMember(archivePath, isZip, member, executablePath),
    );
    if (!extracted) {
      throw ManagedLanguageServerInstallException(
        '${asset.url} does not contain $member.',
      );
    }
    // The mode bit only exists on a POSIX host filesystem.
    if (setExecutableMode &&
        !Platform.isWindows &&
        !setPosixFileMode(executablePath, _executableFileMode)) {
      throw ManagedLanguageServerInstallException(
        'Could not mark $executablePath as executable.',
      );
    }
    return executablePath;
  } finally {
    if (await archiveFile.exists()) {
      await archiveFile.delete();
    }
  }
}

Future<String> _sha256OfFile(String path) async =>
    (await sha256.bind(File(path).openRead()).first).toString();

bool _extractMember(
  String archivePath,
  bool isZip,
  String member,
  String destination,
) {
  final bytes = File(archivePath).readAsBytesSync();
  final archive = isZip
      ? ZipDecoder().decodeBytes(bytes)
      : TarDecoder().decodeBytes(GZipDecoder().decodeBytes(bytes));
  final entry = archive.findFile(member);
  if (entry == null || !entry.isFile) {
    return false;
  }
  File(destination).writeAsBytesSync(entry.readBytes()!, flush: true);
  return true;
}
