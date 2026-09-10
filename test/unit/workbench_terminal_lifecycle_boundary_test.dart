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

  test(
    'sync delegates retired resource cleanup to the application cleaner',
    () {
      final source = File(
        'lib/src/features/workbench/application/workbench_controller_sync.dart',
      ).readAsStringSync();

      expect(source, contains('_retiredResourceCleaner.releaseWorkspace'));
      expect(source, contains('_retiredResourceCleaner.releaseTabs'));
      expect(source, isNot(contains('terminalRuntimeLifecycleProvider')));
      expect(source, isNot(contains('editorSessionRegistryProvider')));
      expect(source, isNot(contains('agentHookReceiverProvider')));
    },
  );

  test(
    'workspace deletion delegates provider cleanup to the deletion cleaner',
    () {
      final source = File(
        'lib/src/features/workbench/application/workbench_controller_projects.dart',
      ).readAsStringSync();

      expect(
        source,
        contains('_deletedWorkspaceResourceCleaner.closeLocalResources'),
      );
      expect(
        source,
        contains('_deletedWorkspaceResourceCleaner.clearObservers'),
      );
      expect(source, isNot(contains('terminalRuntimeLifecycleProvider')));
      expect(source, isNot(contains('editorSessionRegistryProvider')));
      expect(source, isNot(contains('workspaceActivityControllerProvider')));
      expect(source, isNot(contains('agentStatusControllerProvider')));
      expect(source, isNot(contains('agentRuntimeOverlayServiceProvider')));
      expect(source, isNot(contains('agentHookReceiverProvider')));
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

  test('agent notification focus depends on the narrow focus contract', () {
    final source = File(
      'lib/src/features/agent_status/application/agent_status_notification_providers.dart',
    ).readAsStringSync();

    expect(source, contains('terminalRuntimeFocusProvider'));
    expect(source, isNot(contains('terminalRuntimeProvider')));
    expect(source, isNot(contains('.sessionFor(')));
  });

  test('full terminal runtime implements the narrow runtime contracts', () {
    final source = File(
      'lib/src/features/workbench/presentation/terminal_runtime.dart',
    ).readAsStringSync();

    expect(source, contains('abstract interface class TerminalRuntime'));
    expect(source, contains('TerminalRuntimeLifecycle'));
    expect(source, contains('TerminalRuntimeCoordination'));
    expect(source, contains('TerminalRuntimeFocus'));
  });
}
