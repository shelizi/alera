import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:alera/src/core/build_flavor.dart';
import 'package:alera/src/features/updater/domain/alera_update.dart';

/// The fork publishes only these complete Classic desktop distributions.
enum ForkClassicUpdateTarget {
  windowsX64('windows', 'x64', 'wholeDirectoryReplace'),
  macosArm64('macos', 'arm64', 'wholeBundleReplace');

  ForkClassicUpdateTarget(
    this.platform,
    this.architecture,
    this.strategy,
  );

  final String platform;
  final String architecture;
  final String strategy;

  static const repository = 'shelizi/alera';
  static const keyId = 'shelizi-classic-v1';
  static final _stableVersion = RegExp(r'^[0-9]+\.[0-9]+\.[0-9]+$');

  static ForkClassicUpdateTarget? forAbi(Abi abi) => switch (abi) {
    Abi.windowsX64 => windowsX64,
    Abi.macosArm64 => macosArm64,
    _ => null,
  };

  static ForkClassicUpdateTarget? get currentBuild =>
      kIsAleraReleaseFlavor &&
          const bool.fromEnvironment('ALERA_FORK_CLASSIC_UPDATES')
      ? forAbi(Abi.current())
      : null;

  static bool get networkDisabled =>
      Platform.environment['ALERA_DISABLE_UPDATE_CHECKS'] == '1';

  String get slug => '$platform-$architecture';
  String get indexName => 'alera-classic-$slug-update-index.json';
  String get descriptorName => 'alera-classic-$slug-release.json';
  Uri get indexUrl => Uri.parse(
    'https://github.com/$repository/releases/latest/download/$indexName',
  );

  String artifactName(String version) {
    _requireStableVersion(version);
    return 'alera-$version-$slug.zip';
  }

  Uri descriptorUrl(String version) => _assetUrl(version, descriptorName);
  Uri artifactUrl(String version) => _assetUrl(version, artifactName(version));

  Uri _assetUrl(String version, String name) {
    _requireStableVersion(version);
    return Uri.parse(
      'https://github.com/$repository/releases/download/v$version/$name',
    );
  }

  static void _requireStableVersion(String version) {
    if (!_stableVersion.hasMatch(version)) {
      throw const FormatException('Not a stable Classic release version.');
    }
  }

  /// The fork's published stable tags are immutable. A newer build number
  /// must never promote an older (or identical) semantic version.
  static bool isNewerStableVersion(String candidate, String current) {
    _requireStableVersion(candidate);
    _requireStableVersion(current);
    final next = candidate.split('.').map(BigInt.parse).toList();
    final installed = current.split('.').map(BigInt.parse).toList();
    for (var i = 0; i < next.length; i++) {
      final comparison = next[i].compareTo(installed[i]);
      if (comparison != 0) {
        return comparison > 0;
      }
    }
    return false;
  }

  bool allowsDiscovery(AleraUpdateConfig config) =>
      config.archiveUrl == indexUrl &&
      config.channel == AleraUpdateChannel.stable &&
      config.signedRelease &&
      config.manifestPublicKeyId == keyId &&
      validPublicKey(config.manifestPublicKey);

  static bool validPublicKey(String value) {
    try {
      return base64Decode(value.trim()).length == 32;
    } on FormatException {
      return false;
    }
  }
}
