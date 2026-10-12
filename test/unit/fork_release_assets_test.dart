import 'dart:io';

import 'package:archive/archive.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;
import 'package:yaml/yaml.dart';

void main() {
  final bash = Platform.isWindows
      ? r'C:\Program Files\Git\bin\bash.exe'
      : 'bash';
  final script = File('tool/release/verify_fork_release_assets.sh').absolute;
  late Directory directory;

  void asset(String name, List<int> bytes) {
    File(p.join(directory.path, name)).writeAsBytesSync(bytes);
    File(p.join(directory.path, '$name.sha256'))
        .writeAsStringSync('${sha256.convert(bytes)}  $name\n');
  }

  ProcessResult verify() => Process.runSync(bash, <String>[
    script.path.replaceAll(r'\', '/'),
    directory.path.replaceAll(r'\', '/'),
    '1.2.3',
    '4.5.6',
  ]);

  setUp(() {
    directory = Directory.systemTemp.createTempSync('alera-fork-release-');
    final archive = Archive()
      ..addFile(ArchiveFile('payload.txt', 3, <int>[1, 2, 3]));
    final zip = ZipEncoder().encode(archive);
    asset('alera-1.2.3-windows-x64.zip', zip);
    asset(
      'alera-1.2.3-macos-arm64.tar.gz',
      GZipEncoder().encode(TarEncoder().encode(archive)),
    );
    asset('alera-4.5.6-android.apk', zip);
  });

  tearDown(() => directory.deleteSync(recursive: true));

  test('requires all native jobs before fork publication', () {
    final workflow = loadYaml(
      File('.github/workflows/fork-windows-release.yml').readAsStringSync(),
    ) as YamlMap;
    final jobs = workflow['jobs'] as YamlMap;
    final publish = jobs['publish'] as YamlMap;
    expect(
      publish['needs'],
      containsAll(<String>['build-windows', 'build-macos', 'build-android']),
    );
    expect(
      jobs['build-android']['uses'],
      './.github/workflows/fork-android-release.yml',
    );
    expect(
      publish['env']['ANDROID_TARGET_SHA'],
      contains('needs.build-android.outputs.target_sha'),
    );
    expect(
      publish['env']['MOBILE_TAG'],
      contains('needs.build-android.outputs.mobile_tag'),
    );
  });

  test('accepts complete desktop and APK release assets', () {
    final result = verify();
    expect(result.exitCode, 0, reason: result.stderr.toString());
  });

  test('rejects desktop-only publication', () {
    File(p.join(directory.path, 'alera-4.5.6-android.apk')).deleteSync();
    final result = verify();
    expect(result.exitCode, isNot(0));
    expect(result.stdout, contains('Missing alera-4.5.6-android.apk'));
  });

  test('rejects a corrupted APK before publication', () {
    File(p.join(directory.path, 'alera-4.5.6-android.apk'))
        .writeAsBytesSync(<int>[9, 8, 7]);
    final result = verify();
    expect(result.exitCode, isNot(0));
    expect(result.stdout, contains('FAILED'));
  });

  test('rejects unexpected stale assets', () {
    File(p.join(directory.path, 'old.apk')).writeAsStringSync('old build');
    final result = verify();
    expect(result.exitCode, isNot(0));
    expect(result.stdout, contains('Expected 6 release files'));
  });
}
