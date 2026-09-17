import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('production terminal runtime opts into the parser worker', () {
    final source = File(
      'lib/src/features/workbench/presentation/terminal_runtime_providers.dart',
    ).readAsStringSync();

    expect(source, contains('final runtime = XtermTerminalRuntime('));
    expect(source, contains('parserWorkerEnabled: true,'));
  });
}
