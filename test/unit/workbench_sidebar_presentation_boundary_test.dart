import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'sidebar body depends on narrow commands instead of WorkbenchController',
    () {
      final bodySource = File(
        'lib/src/features/workbench/presentation/project_workbench_sidebar_body.dart',
      ).readAsStringSync();
      final shellSource = File(
        'lib/src/features/workbench/presentation/project_workbench_sidebar_shell.dart',
      ).readAsStringSync();
      final collapsedSource = File(
        'lib/src/features/workbench/presentation/project_workbench_collapsed_sidebar.dart',
      ).readAsStringSync();
      final actionsSource = File(
        'lib/src/features/workbench/presentation/project_workbench_sidebar_actions.dart',
      ).readAsStringSync();

      expect(bodySource, contains('_WorkbenchSidebarCommands commands'));
      expect(bodySource, isNot(contains('WorkbenchController controller')));
      expect(bodySource, isNot(contains('controller.')));
      expect(shellSource, contains('_WorkbenchSidebarCommands('));
      expect(collapsedSource, contains('_WorkbenchSidebarCommands commands'));
      expect(
        collapsedSource,
        isNot(contains('WorkbenchController controller')),
      );
      expect(collapsedSource, isNot(contains('controller.')));
      expect(
        actionsSource,
        contains('WorkbenchSidebarTerminalSelectionCoordinator('),
      );
      expect(actionsSource, contains('terminalRuntimeFocusProvider'));
      expect(actionsSource, isNot(contains('terminalRuntimeProvider')));
      expect(actionsSource, isNot(contains('.sessionFor(')));
    },
  );
}
