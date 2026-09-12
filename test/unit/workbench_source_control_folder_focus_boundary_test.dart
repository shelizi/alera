import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('source control folder focus delegates git and filesystem probes to the application service', () {
    final ownerSource = File(
      'lib/src/features/workbench/application/workbench_selection_owner.dart',
    ).readAsStringSync();
    final serviceSource = File(
      'lib/src/features/workbench/application/workbench_source_control_folder_focus_service.dart',
    ).readAsStringSync();

    expect(
      ownerSource,
      contains('WorkbenchSourceControlFolderFocusService('),
    );
    expect(ownerSource, isNot(contains('gitBackendProvider')));
    expect(ownerSource, isNot(contains('Directory(')));
    expect(ownerSource, isNot(contains('File(')));
    expect(ownerSource, isNot(contains('_hasDirectGitEntry')));
    expect(
      serviceSource,
      contains('WorkbenchGitRepositoryProbe _gitRepositoryProbe'),
    );
    expect(serviceSource, contains('_hasDirectGitEntry(path)'));
    expect(serviceSource, isNot(contains('gitBackendProvider')));
  });
}
