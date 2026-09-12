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

  test('sync delegates retired resource cleanup to the application cleaner', () {
    final source = File(
      'lib/src/features/workbench/application/workbench_controller_sync.dart',
    ).readAsStringSync();
    final ownerSource = File(
      'lib/src/features/workbench/application/workbench_tab_layout_owner.dart',
    ).readAsStringSync();

    expect(source, contains('_retiredWorkspaceCleanup.cleanup'));
    expect(source, isNot(contains('_retiredResourceCleaner.releaseWorkspace')));
    expect(ownerSource, contains('retiredTabsCleanup.cleanup'));
    expect(ownerSource, isNot(contains('_retiredResourceCleaner.releaseTabs')));
    expect(source, isNot(contains('terminalRuntimeLifecycleProvider')));
    expect(source, isNot(contains('editorSessionRegistryProvider')));
    expect(source, isNot(contains('agentHookReceiverProvider')));
    expect(ownerSource, isNot(contains('terminalRuntimeLifecycleProvider')));
    expect(ownerSource, isNot(contains('editorSessionRegistryProvider')));
    expect(ownerSource, isNot(contains('agentHookReceiverProvider')));
  });

  test(
    'workspace deletion delegates provider cleanup to the deletion coordinator',
    () {
      final controllerSource = File(
        'lib/src/features/workbench/application/workbench_controller_projects.dart',
      ).readAsStringSync();
      final coordinatorSource = File(
        'lib/src/features/workbench/application/workbench_delete_workspace_cleanup_coordinator.dart',
      ).readAsStringSync();

      expect(
        controllerSource,
        contains('WorkbenchDeleteWorkspaceCleanupCoordinator('),
      );
      expect(
        controllerSource,
        isNot(
          contains('_explicitResourceCleaner.closeWorkspaceLocalResources'),
        ),
      );
      expect(
        controllerSource,
        isNot(
          contains('_explicitResourceCleaner.clearDeletedWorkspaceObservers'),
        ),
      );
      expect(
        coordinatorSource,
        contains('_resourceCleaner.closeWorkspaceLocalResources'),
      );
      expect(
        coordinatorSource,
        contains('_resourceCleaner.clearDeletedWorkspaceObservers'),
      );
      expect(
        controllerSource,
        isNot(contains('terminalRuntimeLifecycleProvider')),
      );
      expect(
        controllerSource,
        isNot(contains('editorSessionRegistryProvider')),
      );
      expect(
        controllerSource,
        isNot(contains('workspaceActivityControllerProvider')),
      );
      expect(
        controllerSource,
        isNot(contains('agentStatusControllerProvider')),
      );
      expect(
        controllerSource,
        isNot(contains('agentRuntimeOverlayServiceProvider')),
      );
      expect(controllerSource, isNot(contains('agentHookReceiverProvider')));
    },
  );

  test(
    'tab close delegates explicit local cleanup to the close coordinator',
    () {
      final controllerSource = File(
        'lib/src/features/workbench/application/workbench_tab_layout_owner_tabs.dart',
      ).readAsStringSync();
      final coordinatorSource = File(
        'lib/src/features/workbench/application/workbench_tab_close_coordinator.dart',
      ).readAsStringSync();

      expect(controllerSource, contains('WorkbenchTabCloseCoordinator('));
      expect(
        controllerSource,
        isNot(contains('_explicitResourceCleaner.closeTabLocalResources')),
      );
      expect(
        coordinatorSource,
        contains('_resourceCleaner.closeTabLocalResources'),
      );
      expect(
        controllerSource,
        isNot(contains('terminalRuntimeLifecycleProvider')),
      );
      expect(
        controllerSource,
        isNot(contains('editorSessionRegistryProvider')),
      );
    },
  );

  test(
    'workbench application no longer composes the full terminal runtime',
    () {
      final source = File(
        'lib/src/features/workbench/application/workbench_providers.dart',
      ).readAsStringSync();

      expect(source, isNot(contains('/presentation/')));
      expect(source, isNot(contains('TerminalRuntime terminalRuntime(')));
      expect(source, isNot(contains('terminalRuntimeProvider')));
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
    expect(source, contains('implements TerminalRuntimeBindings'));
  });
}
