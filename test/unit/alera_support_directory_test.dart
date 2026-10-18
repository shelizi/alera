import 'dart:io';

import 'package:alera/src/shared/infra/storage/alera_support_directory.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  test('normal builds have no support-directory override', () {
    expect(aleraTestSupportDirectory(''), isNull);
  });

  test('smoke builds use only an absolute isolated support directory', () {
    final dir = p.join(Directory.systemTemp.path, 'alera', '..', 'alera-smoke');
    expect(aleraTestSupportDirectory(dir)!.path, p.normalize(dir));
    expect(
      () => aleraTestSupportDirectory('relative/data'),
      throwsArgumentError,
    );
  });
}
