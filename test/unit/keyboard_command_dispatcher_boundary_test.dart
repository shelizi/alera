import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('UI keyboard command dispatcher lives in presentation', () {
    expect(
      File(
        'lib/src/features/keyboard/application/keyboard_command_dispatcher.dart',
      ).existsSync(),
      isFalse,
    );
    expect(
      File(
        'lib/src/features/keyboard/presentation/keyboard_command_dispatcher.dart',
      ).existsSync(),
      isTrue,
    );
  });
}
