import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'workbench controllers depend only on terminal lifecycle operations',
    () {
      const controllerFiles = <String>[
        'lib/src/features/workbench/application/workbench_controller_sync.dart',
        'lib/src/features/workbench/application/workbench_controller_tabs.dart',
        'lib/src/features/workbench/application/workbench_controller_projects.dart',
      ];

      for (final path in controllerFiles) {
        final source = File(path).readAsStringSync();
        expect(
          source,
          isNot(contains('terminalRuntimeProvider')),
          reason: '$path must not depend on the full terminal UI runtime.',
        );
      }
    },
  );

  test('full terminal runtime implements the lifecycle contract', () {
    final source = File(
      'lib/src/features/workbench/presentation/terminal_runtime.dart',
    ).readAsStringSync();

    expect(
      source,
      contains(
        'abstract interface class TerminalRuntime implements TerminalRuntimeLifecycle',
      ),
    );
  });
}
