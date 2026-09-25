import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// The one file allowed to spell Windows path prefixes.
const String _pathIdentityOwner =
    'lib/src/shared/infra/files/path_identity.dart';

// Matches the verbatim `\\?\` and device `\\.\` prefixes written either as a
// raw string (`r'\\?\'`) or with escapes (`'\\\\?\\'`).
final RegExp _prefixLiteral = RegExp(r"\\\\[?.]\\|\\\\\\\\[?.]\\\\");

void main() {
  test('only path_identity.dart handles Windows verbatim prefixes', () {
    final offenders = <String>[];

    for (final entity in Directory('lib').listSync(recursive: true)) {
      if (entity is! File || !entity.path.endsWith('.dart')) {
        continue;
      }
      final path = entity.path.replaceAll(r'\', '/');
      // Generated bridge code is not hand-written app code.
      if (path.startsWith('lib/src/rust/') || path == _pathIdentityOwner) {
        continue;
      }
      final lines = entity.readAsLinesSync();
      for (var index = 0; index < lines.length; index += 1) {
        final line = lines[index];
        if (line.trimLeft().startsWith('//')) {
          continue;
        }
        if (_prefixLiteral.hasMatch(line)) {
          offenders.add('$path:${index + 1}: ${line.trim()}');
        }
      }
    }

    expect(
      offenders,
      isEmpty,
      reason:
          'These lines handle a Windows verbatim or device path prefix outside '
          '$_pathIdentityOwner. Hand-written copies drift: they disagree about '
          'UNC and device paths and about whether POSIX hosts are affected. '
          'Use withoutWindowsPathPrefix, comparablePath, isSamePath, '
          'isPathWithinOrSame or relativePathWithin instead.\n'
          '${offenders.join('\n')}',
    );
  });

  test('the owner still recognizes every prefix the runtime can emit', () {
    final source = File(_pathIdentityOwner).readAsStringSync();
    for (final prefix in const <String>[r'\\?\UNC\', r'\\?\', r'\\.\']) {
      expect(source, contains(prefix), reason: 'missing $prefix handling');
    }
  });
}
