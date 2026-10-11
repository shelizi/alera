import 'package:alera_mobile/src/features/workbench/application/mobile_workspace_filename.dart';
import 'package:test/test.dart';

void main() {
  test('rejects path traversal, ADS, DOS devices and host temp filenames', () {
    for (final name in <String>[
      '',
      '.',
      '..',
      '../secret',
      'a\\b',
      'a:b',
      'CON',
      'con.txt',
      'AUX.md',
      'COM1.txt',
      'LPT9',
      'bad?.md',
      'ending.',
      '.git',
      '.alera-mobile-edit-test.tmp',
      'line\nfeed',
      ' spaced',
    ]) {
      expect(isSafeMobileWorkspaceFileName(name), isFalse, reason: name);
    }
  });

  test('accepts Unicode, hidden and ordinary text filenames', () {
    for (final name in <String>['readme.md', '.env', '台灣.md', 'a-b_c.txt']) {
      expect(isSafeMobileWorkspaceFileName(name), isTrue, reason: name);
    }
  });
}
