import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/workbench/application/workspace_git_repositories.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
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

  test(
    'overview batches repository reads and separates their states',
    () async {
      final waiting = <String, Completer<void>>{};
      final requests = <String>[];
      final stream = loadGitRepositoryOverview(
        workspacePath: root.path,
        roots: Future.value(<String>['', 'frontend', 'backend', 'tools/cli']),
        batchSize: 2,
        loadStatus: (path) async {
          requests.add(path);
          final relative = p.relative(path, from: root.path);
          final gate = waiting.putIfAbsent(relative, Completer<void>.new);
          await gate.future;
          return GitStatusResult(
            entries: <GitChangeEntry>[
              GitChangeEntry(
                path: relative == '.' ? 'README.md' : 'lib/main.dart',
                area: .unstaged,
                status: .modified,
              ),
            ],
          );
        },
        loadRepositoryState: (path) async {
          final relative = p.relative(path, from: root.path);
          return GitRepositoryState(
            branch: relative == '.' ? 'main' : relative,
          );
        },
      );
      final emitted = <Map<String, GitRepositoryOverviewSummary>>[];
      final done = Completer<void>();
      final subscription = stream.listen(emitted.add, onDone: done.complete);
      await Future<void>.delayed(const Duration(milliseconds: 10));
      expect(requests, hasLength(2));
      expect(emitted, isEmpty);
      waiting['.']!.complete();
      waiting['frontend']!.complete();
      await Future<void>.delayed(const Duration(milliseconds: 10));
      expect(emitted, hasLength(1));
      expect(emitted.first.keys.toSet(), <String>{'', 'frontend'});
      expect(emitted.first['']?.branch, 'main');
      expect(emitted.first['frontend']?.changedFiles, 1);
      expect(requests, hasLength(4));
      waiting['backend']!.complete();
      waiting[p.join('tools', 'cli')]!.complete();
      await done.future;
      expect(emitted.last.keys.toSet(), <String>{
        '',
        'frontend',
        'backend',
        'tools/cli',
      });
      await subscription.cancel();
    },
  );

  test(
    'overview isolates failed repositories and supports cancellation',
    () async {
      final emitted = await loadGitRepositoryOverview(
        workspacePath: root.path,
        roots: Future.value(<String>['ok', 'broken', 'skipped']),
        batchSize: 2,
        loadStatus: (path) async {
          if (path.endsWith('broken')) throw StateError('bad repository');
          return const GitStatusResult(entries: <GitChangeEntry>[]);
        },
        loadRepositoryState: (path) async =>
            const GitRepositoryState(branch: 'main', ahead: 1),
        isCancelled: () => false,
      ).toList();
      expect(emitted, hasLength(2));
      expect(emitted.last['broken']?.available, isFalse);
      expect(emitted.last['ok']?.ahead, 1);
      expect(emitted.last['skipped']?.branch, 'main');

      var canceled = false;
      final stopped = await loadGitRepositoryOverview(
        workspacePath: root.path,
        roots: Future.value(<String>['a', 'b']),
        batchSize: 1,
        loadStatus: (_) async {
          canceled = true;
          return const GitStatusResult(entries: <GitChangeEntry>[]);
        },
        loadRepositoryState: (_) async =>
            const GitRepositoryState(branch: 'main'),
        isCancelled: () => canceled,
      ).toList();
      expect(stopped, isEmpty);
    },
  );

  test('returns empty for a missing workspace', () async {
    expect(
      await const WorkspaceGitRepositoryScanner().scan(
        p.join(root.path, 'missing'),
      ),
      isEmpty,
    );
  });
}
