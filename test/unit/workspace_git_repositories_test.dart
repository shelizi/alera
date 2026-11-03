import 'dart:io';

import 'package:alera/src/features/workbench/application/workspace_git_repositories.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  late Directory root;

  setUp(() async {
    root = await Directory.systemTemp.createTemp('alera-repo-scan-');
  });
  tearDown(() async {
    if (await root.exists()) await root.delete(recursive: true);
  });

  Future<void> createRepo(String relativePath, {bool worktree = false}) async {
    final directory = relativePath.isEmpty
        ? root
        : await Directory(p.join(root.path, relativePath))
              .create(recursive: true);
    if (worktree) {
      await File(p.join(directory.path, '.git'))
          .writeAsString('gitdir: ../.git/worktrees/example');
    } else {
      await Directory(p.join(directory.path, '.git')).create();
    }
  }

  test('finds multiple independent, nested and worktree Git roots', () async {
    await createRepo('');
    await createRepo(p.join('packages', 'frontend'));
    await createRepo(p.join('packages', 'backend'), worktree: true);
    await createRepo(p.join('packages', 'frontend', 'nested'));
    await createRepo(p.join('tools', 'cli'));

    expect(
      await const WorkspaceGitRepositoryScanner().scan(root.path),
      <String>[
        '',
        'packages/backend',
        'packages/frontend',
        'packages/frontend/nested',
        'tools/cli',
      ],
    );
  });

  test('does not traverse generated caches or linked worktrees', () async {
    await createRepo(p.join('packages', 'app'));
    await createRepo(p.join('node_modules', 'package'));
    await createRepo(p.join('.worktrees', 'other'));
    await createRepo(p.join('build', 'output'));
    expect(
      await const WorkspaceGitRepositoryScanner().scan(root.path),
      <String>['packages/app'],
    );
  });

  test('respects depth and directory scan bounds', () async {
    await createRepo(p.join('one', 'two'));
    await createRepo('top');
    expect(
      await const WorkspaceGitRepositoryScanner(maxDepth: 1).scan(root.path),
      <String>['top'],
    );
    expect(
      await const WorkspaceGitRepositoryScanner(maxDirectories: 1)
          .scan(root.path),
      isEmpty,
    );
  });

  test('returns empty for a missing workspace', () async {
    expect(
      await const WorkspaceGitRepositoryScanner().scan(
        p.join(root.path, 'missing'),
      ),
      isEmpty,
    );
  });
}
