import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('source control folder focus delegates git and filesystem probes to the application service', () {
    final controllerSource = File(
      'lib/src/features/workbench/application/workbench_controller_view_prefs.dart',
    ).readAsStringSync();
    final serviceSource = File(
      'lib/src/features/workbench/application/workbench_source_control_folder_focus_service.dart',
    ).readAsStringSync();

    expect(
      controllerSource,
      contains('WorkbenchSourceControlFolderFocusService('),
    );
    expect(controllerSource, isNot(contains('gitBackendProvider')));
    expect(controllerSource, isNot(contains('Directory(')));
    expect(controllerSource, isNot(contains('File(')));
    expect(controllerSource, isNot(contains('_hasDirectGitEntry')));
    expect(
      serviceSource,
      contains('WorkbenchGitRepositoryProbe _gitRepositoryProbe'),
    );
    expect(serviceSource, contains('_hasDirectGitEntry(path)'));
    expect(serviceSource, isNot(contains('gitBackendProvider')));
  });
}
