import 'dart:io';

import 'package:alera/src/features/workbench/presentation/workspace_git_ignore.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('escapes gitignore metacharacters in repo-relative file paths', () {
    expect(
      workspaceGitIgnorePattern(
        'lib/src/dirty file[1].dart',
        isDirectory: false,
      ),
      r'/lib/src/dirty\ file\[1\].dart',
    );
  });

  test('directory patterns are rooted and end with a slash', () {
    expect(
      workspaceGitIgnorePattern('lib/src', isDirectory: true),
      '/lib/src/',
    );
  });

  test('adding the same path twice is idempotent', () async {
    final directory = await Directory.systemTemp.createTemp(
      'alera-gitignore-helper-',
    );
    addTearDown(() => directory.delete(recursive: true));

    expect(
      await addWorkspacePathToGitIgnore(
        rootPath: directory.path,
        path: 'lib/src/dirty file.dart',
        isDirectory: false,
      ),
      isTrue,
    );
    expect(
      await addWorkspacePathToGitIgnore(
        rootPath: directory.path,
        path: 'lib/src/dirty file.dart',
        isDirectory: false,
      ),
      isFalse,
    );

    final gitIgnore = File(
      '${directory.path}${Platform.pathSeparator}.gitignore',
    );
    expect(await gitIgnore.readAsString(), '/lib/src/dirty\\ file.dart\n');
  });
}
