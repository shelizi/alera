import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/updater/domain/fork_classic_update_target.dart';
import 'package:archive/archive.dart';
import 'package:crypto/crypto.dart';
import 'package:cryptography/cryptography.dart';
import 'package:path/path.dart' as p;

/// Generate or verify schema v3 metadata for one complete Classic artifact.
/// The signing key is read only from the process environment.
Future<void> main(List<String> args) async {
  if (args.length != 4 || !const {'generate', 'verify'}.contains(args.first)) {
    stderr.writeln(
      'Usage: dart tool/release/fork_classic_update_metadata.dart '
      '<generate|verify> <windows-x64|macos-arm64> <version+build> <dist>',
    );
    exitCode = 64;
    return;
  }
  final target = ForkClassicUpdateTarget.values.singleWhere(
    (candidate) => candidate.slug == args[1],
  );
  final parts = args[2].split('+');
  if (parts.length != 2 || int.tryParse(parts[1]) == null) {
    throw const FormatException(
      'A stable version and build number are required.',
    );
  }
  final version = parts.first;
  final build = int.parse(parts[1]);
  final directory = Directory(args[3]);
  final publicKey = Platform.environment['ALERA_FORK_UPDATE_PUBLIC_KEY'] ?? '';
  if (args.first == 'generate') {
    await generateForkClassicUpdateMetadata(
      target: target,
      version: version,
      buildNumber: build,
      directory: directory,
      privateSeed: Platform.environment['ALERA_FORK_UPDATE_PRIVATE_KEY'] ?? '',
      publicKey: publicKey,
    );
  }
  await verifyForkClassicUpdateMetadata(
    target: target,
    version: version,
    buildNumber: build,
    directory: directory,
    publicKey: publicKey,
  );
  stdout.writeln('Verified Classic fork update metadata for ${target.slug}.');
}

Future<void> generateForkClassicUpdateMetadata({
  required ForkClassicUpdateTarget target,
  required String version,
  required int buildNumber,
  required Directory directory,
  required String privateSeed,
  required String publicKey,
}) async {
  if (!ForkClassicUpdateTarget.validPublicKey(publicKey) ||
      !ForkClassicUpdateTarget.validPublicKey(privateSeed)) {
    throw const FormatException(
      'Fork update keys must be raw 32-byte base64 keys.',
    );
  }
  final algorithm = Ed25519();
  final pair = await algorithm.newKeyPairFromSeed(
    base64Decode(privateSeed.trim()),
  );
  final derived = await pair.extractPublicKey();
  if (base64Encode(derived.bytes) !=
      base64Encode(base64Decode(publicKey.trim()))) {
    throw const FormatException(
      'Fork update public key does not match the seed.',
    );
  }
  final artifact = await _artifactMetadata(target, version, directory);
  final descriptor = _descriptor(
    target,
    version,
    buildNumber,
    artifact,
    DateTime.now().toUtc().toIso8601String(),
  );
  final signature = await algorithm.sign(_canonical(descriptor), keyPair: pair);
  final signed = Map<String, Object?>.from(descriptor)
    ..['signature'] = {
      'algorithm': 'ed25519',
      'publicKeyId': ForkClassicUpdateTarget.keyId,
      'value': base64Encode(signature.bytes),
    };
  await _write(directory, target.descriptorName, signed);
  await _write(
    directory,
    target.indexName,
    _index(target, version, buildNumber),
  );
}

Future<void> verifyForkClassicUpdateMetadata({
  required ForkClassicUpdateTarget target,
  required String version,
  required int buildNumber,
  required Directory directory,
  required String publicKey,
}) async {
  if (!ForkClassicUpdateTarget.validPublicKey(publicKey)) {
    throw const FormatException('A valid fork update public key is required.');
  }
  final index = await _read(directory, target.indexName);
  if (jsonEncode(_sort(index)) !=
      jsonEncode(_sort(_index(target, version, buildNumber)))) {
    throw const FormatException('Fork update index identity mismatch.');
  }
  final descriptor = await _read(directory, target.descriptorName);
  final signature = descriptor['signature'];
  if (signature is! Map ||
      signature['algorithm'] != 'ed25519' ||
      signature['publicKeyId'] != ForkClassicUpdateTarget.keyId ||
      signature['value'] is! String) {
    throw const FormatException('Fork update signature is missing or invalid.');
  }
  final generatedAt = descriptor['generatedAt'];
  if (generatedAt is! String || DateTime.tryParse(generatedAt) == null) {
    throw const FormatException('Invalid generation time.');
  }
  final expected = _descriptor(
    target,
    version,
    buildNumber,
    await _artifactMetadata(target, version, directory),
    generatedAt,
  );
  final unsigned = Map<String, Object?>.from(descriptor)
    ..['signature'] = {
      'algorithm': 'ed25519',
      'publicKeyId': ForkClassicUpdateTarget.keyId,
      'value': '',
    };
  if (jsonEncode(_sort(expected)) != jsonEncode(_sort(unsigned))) {
    throw const FormatException('Fork update descriptor or artifact mismatch.');
  }
  final verified = await Ed25519().verify(
    _canonical(unsigned),
    signature: Signature(
      base64Decode(signature['value'] as String),
      publicKey: SimplePublicKey(
        base64Decode(publicKey.trim()),
        type: .ed25519,
      ),
    ),
  );
  if (!verified) {
    throw const FormatException('Fork update signature verification failed.');
  }
}

Future<Map<String, Object?>> _artifactMetadata(
  ForkClassicUpdateTarget target,
  String version,
  Directory directory,
) async {
  final file = File(p.join(directory.path, target.artifactName(version)));
  final length = await file.length();
  if (length == 0) {
    throw const FormatException('The Classic update artifact is empty.');
  }
  await _verifyArchiveLayout(target, file);
  return {
    'kind': 'zip',
    'url': target.artifactUrl(version).toString(),
    'sha256': (await sha256.bind(file.openRead()).first).toString(),
    'length': length,
  };
}

Future<void> _verifyArchiveLayout(
  ForkClassicUpdateTarget target,
  File file,
) async {
  final archive = ZipDecoder().decodeBytes(await file.readAsBytes());
  final names = <String>{};
  for (final entry in archive.files) {
    final name = entry.name;
    if (name == target.descriptorAppName ||
        name == '${target.descriptorAppName}/') {
      if (target == ForkClassicUpdateTarget.macosArm64 &&
          (!entry.isDirectory || entry.isSymbolicLink)) {
        throw const FormatException(
          'Classic macOS bundle root is not a real directory.',
        );
      }
    }
    if (entry.isFile && !entry.isSymbolicLink && entry.size > 0) {
      if (!names.add(name)) {
        throw const FormatException('Duplicate Classic ZIP file.');
      }
    }
  }
  final required = target == ForkClassicUpdateTarget.macosArm64
      ? [
          '${target.descriptorAppName}/Contents/Info.plist',
          '${target.descriptorAppName}/Contents/MacOS/Alera',
        ]
      : [
          'Alera.exe',
          'flutter_windows.dll',
          'desktop_updater_plugin.dll',
          'resources/alera/alera.exe',
        ];
  if (!names.containsAll(required)) {
    throw FormatException(
      'Classic ZIP does not match ${target.descriptorAppName}: missing ${required.where((name) => !names.contains(name)).join(', ')}',
    );
  }
}

Map<String, Object?> _descriptor(
  ForkClassicUpdateTarget target,
  String version,
  int buildNumber,
  Map<String, Object?> artifact,
  String generatedAt,
) => {
  'schemaVersion': 3,
  'packageId': 'dev.leynier.alera',
  'appName': target.descriptorAppName,
  'version': version,
  'buildNumber': buildNumber,
  'platform': target.platform,
  'channel': 'stable',
  'artifact': artifact,
  'install': {'strategy': target.strategy},
  'minimumUpdaterVersion': '2.7.0',
  'generatedAt': generatedAt,
  'signature': {
    'algorithm': 'ed25519',
    'publicKeyId': ForkClassicUpdateTarget.keyId,
    'value': '',
  },
};

Map<String, Object?> _index(
  ForkClassicUpdateTarget target,
  String version,
  int buildNumber,
) => {
  'schemaVersion': 3,
  'appName': 'Alera',
  'items': [
    {
      'version': version,
      'buildNumber': buildNumber,
      'platform': target.platform,
      'channel': 'stable',
      'mandatory': false,
      'release': target.descriptorUrl(version).toString(),
    },
  ],
};

List<int> _canonical(Map<String, Object?> descriptor) =>
    utf8.encode(jsonEncode(_sort(descriptor)));

Object? _sort(Object? value) {
  if (value is Map) {
    final keys = value.keys.cast<String>().toList()..sort();
    return {for (final key in keys) key: _sort(value[key])};
  }
  if (value is List) {
    return value.map(_sort).toList();
  }
  return value;
}

Future<Map<String, Object?>> _read(Directory directory, String name) async =>
    Map<String, Object?>.from(
      jsonDecode(await File(p.join(directory.path, name)).readAsString())
          as Map,
    );

Future<void> _write(
  Directory directory,
  String name,
  Map<String, Object?> value,
) => File(p.join(directory.path, name))
    .writeAsString('${const JsonEncoder.withIndent('  ').convert(value)}\n');
