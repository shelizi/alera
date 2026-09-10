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

  test('runtime coordinators depend on the narrow coordination contract', () {
    final source = File(
      'lib/src/features/workbench/application/workbench_providers.dart',
    ).readAsStringSync();

    final activeWorkspaceCoordinator = source.substring(
      source.indexOf('void terminalRuntimeActiveWorkspaceCoordinator'),
      source.indexOf('WorkspaceActivityRepository workspaceActivityRepository'),
    );
    final exitCoordinator = source.substring(
      source.indexOf('void terminalRuntimeExitCoordinator'),
    );

    expect(
      activeWorkspaceCoordinator,
      isNot(contains('terminalRuntimeProvider')),
    );
    expect(exitCoordinator, isNot(contains('terminalRuntimeProvider')));
  });

  test('full terminal runtime implements the narrow runtime contracts', () {
    final source = File(
      'lib/src/features/workbench/presentation/terminal_runtime.dart',
    ).readAsStringSync();

    expect(source, contains('abstract interface class TerminalRuntime'));
    expect(
      source,
      contains(
        'implements TerminalRuntimeLifecycle, TerminalRuntimeCoordination',
      ),
    );
  });
}
