import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('replaceable file tab orchestration uses the narrow editor-session contract', () {
    final controllerSource = File(
      'lib/src/features/workbench/application/workbench_controller_file_tabs.dart',
    ).readAsStringSync();
    final coordinatorSource = File(
      'lib/src/features/workbench/application/workbench_replaceable_tab_open_coordinator.dart',
    ).readAsStringSync();

    expect(
      controllerSource,
      contains('WorkbenchReplaceableTabOpenCoordinator('),
    );
    expect(controllerSource, isNot(contains('editorSessionRegistryProvider')));
    expect(controllerSource, isNot(contains('workbenchPreviewTabIdInGroup(')));
    expect(controllerSource, isNot(contains('shouldForgetEditorSession')));
    expect(
      coordinatorSource,
      contains('WorkbenchReplaceableTabEditorSessions _editorSessions'),
    );
    expect(coordinatorSource, isNot(contains('editorSessionRegistryProvider')));
  });
}
