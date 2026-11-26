import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/workbench/application/workspace_git_repositories.dart';
import 'package:alera/src/features/workbench/application/workspace_git_repository_change_count.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import 'fake_git_backend.dart';

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

  test(
    'finds child Git roots when the workspace root is a plain folder',
    () async {
      await createRepo('frontend');
      await createRepo(p.join('backend', 'service'));
      await createRepo(p.join('tools', 'cli'), worktree: true);

      expect(await Directory(p.join(root.path, '.git')).exists(), isFalse);
      expect(
        await const WorkspaceGitRepositoryScanner().scan(root.path),
        <String>['backend/service', 'frontend', 'tools/cli'],
      );
    },
  );

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
        loadChangedFileCount: (path) async {
          requests.add(path);
          final relative = p.relative(path, from: root.path);
          final gate = waiting.putIfAbsent(relative, Completer<void>.new);
          await gate.future;
          return 1;
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
        loadChangedFileCount: (path) async {
          if (path.endsWith('broken')) throw StateError('bad repository');
          return 0;
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
        loadChangedFileCount: (_) async {
          canceled = true;
          return 0;
        },
        loadRepositoryState: (_) async =>
            const GitRepositoryState(branch: 'main'),
        isCancelled: () => canceled,
      ).toList();
      expect(stopped, isEmpty);
    },
  );

  test(
    'overview count uses lightweight paths and falls back only for null',
    () async {
      var lightweightCalls = 0;
      var fullCalls = 0;
      List<String>? changed = <String>['tracked.txt', 'new.txt'];
      Future<int> count() => loadWorkspaceGitRepositoryChangeCount(
        path: root.path,
        loadChangedPaths: (path) async {
          expect(path, root.path);
          lightweightCalls++;
          return changed;
        },
        loadStatus: (path) async {
          expect(path, root.path);
          fullCalls++;
          return const GitStatusResult(
            entries: [
              GitChangeEntry(
                path: 'modules/sample',
                area: .staged,
                status: .modified,
              ),
              GitChangeEntry(
                path: 'modules/sample',
                area: .unstaged,
                status: .modified,
              ),
              GitChangeEntry(
                path: 'other.txt',
                area: .untracked,
                status: .untracked,
              ),
            ],
          );
        },
      );

      expect(await count(), 2);
      changed = <String>[];
      expect(await count(), 0);
      expect(fullCalls, 0);
      changed = null;
      expect(await count(), 2);
      expect(fullCalls, 1);
      changed = ['latest.txt'];
      expect(await count(), 1);
      expect(lightweightCalls, 4);
      expect(fullCalls, 1);
    },
  );

  test(
    'overview does not repeat a failed authoritative lightweight query',
    () async {
      var fullCalls = 0;
      final error = StateError('status failed');
      await expectLater(
        loadWorkspaceGitRepositoryChangeCount(
          path: root.path,
          loadChangedPaths: (_) async => throw error,
          loadStatus: (_) async {
            fullCalls++;
            return const GitStatusResult(entries: []);
          },
        ),
        throwsA(same(error)),
      );
      expect(fullCalls, 0);
    },
  );

  test(
    'overview provider uses lightweight query for all 100 repository roots',
    () async {
      final backend = FakeGitBackend()
        ..gitStatusResult = const GitStatusResult(
          entries: [
            GitChangeEntry(
              path: 'tracked.txt',
              area: .staged,
              status: .modified,
            ),
            GitChangeEntry(
              path: 'tracked.txt',
              area: .unstaged,
              status: .modified,
            ),
            GitChangeEntry(
              path: 'new.txt',
              area: .untracked,
              status: .untracked,
            ),
          ],
        );
      final roots = List<String>.generate(100, (i) => 'repo-$i');
      final container = ProviderContainer(
        overrides: [
          gitBackendProvider.overrideWithValue(backend),
          workspaceGitRepositoriesProvider(root.path)
              .overrideWith((ref) async => roots),
        ],
      );
      addTearDown(container.dispose);
      final complete = Completer<Map<String, GitRepositoryOverviewSummary>>();
      final provider = workspaceGitRepositoryOverviewProvider(root.path);
      final subscription = container.listen(provider, (_, next) {
        if (next.hasError && !complete.isCompleted) {
          complete.completeError(next.error!, next.stackTrace);
        }
        final data = next.value;
        if (data != null &&
            data.length == roots.length &&
            !complete.isCompleted) {
          complete.complete(data);
        }
      });
      addTearDown(subscription.close);
      final summaries = await complete.future;
      expect(summaries.length, 100);
      expect(
        summaries.values.every(
          (summary) => summary.available && summary.changedFiles == 2,
        ),
        isTrue,
      );
      expect(
        backend.calls.where((call) => call.method == 'diffChangedPaths'),
        hasLength(100),
      );
      expect(backend.calls.where((call) => call.method == 'status'), isEmpty);
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
