import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/workbench/application/git_worktree_metadata_watcher.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  late Directory tempDir;

  setUp(() {
    tempDir = Directory.systemTemp.createTempSync('alera-worktree-watcher-');
  });

  tearDown(() {
    if (tempDir.existsSync()) {
      tempDir.deleteSync(recursive: true);
    }
  });

  test('resolveGitCommonDir handles main and linked worktrees', () {
    final mainRepo = Directory(p.join(tempDir.path, 'main'))..createSync();
    final mainGit = Directory(p.join(mainRepo.path, '.git'))..createSync();
    expect(resolveGitCommonDir(mainRepo.path), _absolute(mainGit.path));

    final commonGit = Directory(p.join(tempDir.path, 'common.git'))
      ..createSync();
    final linkedGitDir = Directory(
      p.join(commonGit.path, 'worktrees', 'linked'),
    )..createSync(recursive: true);
    File(p.join(linkedGitDir.path, 'commondir')).writeAsStringSync('../..\n');
    final linkedRepo = Directory(p.join(tempDir.path, 'linked'))..createSync();
    File(p.join(linkedRepo.path, '.git')).writeAsStringSync(
      'gitdir: ${p.relative(linkedGitDir.path, from: linkedRepo.path)}\n',
    );

    expect(resolveGitCommonDir(linkedRepo.path), _absolute(commonGit.path));
  });

  test(
    'metadata events are debounced and unrelated git changes are ignored',
    () async {
      final repo = Directory(p.join(tempDir.path, 'repo'))..createSync();
      final gitDir = Directory(p.join(repo.path, '.git'))..createSync();
      final worktreesDir = Directory(p.join(gitDir.path, 'worktrees'))
        ..createSync();
      final watches = _FakeWatchFactory();
      var refreshes = 0;
      final watcher = GitWorktreeMetadataWatcher(
        repoPath: repo.path,
        onRefresh: () async => refreshes += 1,
        debounce: const Duration(milliseconds: 8),
        pollInterval: const Duration(hours: 1),
        watchDirectory: watches.call,
      );
      addTearDown(() async {
        await watcher.dispose();
        await watches.dispose();
      });

      watcher.start();
      expect(watches.isWatching(gitDir.path, recursive: false), isTrue);
      expect(watches.isWatching(worktreesDir.path, recursive: true), isTrue);

      watches.emit(worktreesDir.path, p.join(worktreesDir.path, 'ext', 'HEAD'));
      watches.emit(
        worktreesDir.path,
        p.join(worktreesDir.path, 'ext', 'gitdir'),
      );
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(refreshes, 1);

      watches.emit(gitDir.path, p.join(gitDir.path, 'FETCH_HEAD'));
      await Future<void>.delayed(const Duration(milliseconds: 20));
      expect(refreshes, 1);

      watches.emit(gitDir.path, worktreesDir.path);
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(refreshes, 2);
    },
  );

  test(
    'suspended refresh waits until an internal worktree mutation finishes',
    () async {
      final repo = Directory(p.join(tempDir.path, 'repo'))..createSync();
      final gitDir = Directory(p.join(repo.path, '.git'))..createSync();
      final worktreesDir = Directory(p.join(gitDir.path, 'worktrees'))
        ..createSync();
      final watches = _FakeWatchFactory();
      var refreshes = 0;
      final watcher = GitWorktreeMetadataWatcher(
        repoPath: repo.path,
        onRefresh: () async => refreshes += 1,
        debounce: const Duration(milliseconds: 5),
        pollInterval: const Duration(hours: 1),
        watchDirectory: watches.call,
      );
      addTearDown(() async {
        await watcher.dispose();
        await watches.dispose();
      });

      watcher.start();
      await watcher.suspendRefresh();
      watches.emit(worktreesDir.path, p.join(worktreesDir.path, 'internal'));
      await Future<void>.delayed(const Duration(milliseconds: 20));
      expect(refreshes, 0);

      watcher.resumeRefresh();
      await Future<void>.delayed(const Duration(milliseconds: 25));
      expect(refreshes, 1);
    },
  );

  test('suspend waits for an already running refresh', () async {
    final repo = Directory(p.join(tempDir.path, 'repo'))..createSync();
    final gitDir = Directory(p.join(repo.path, '.git'))..createSync();
    final worktreesDir = Directory(p.join(gitDir.path, 'worktrees'))
      ..createSync();
    final watches = _FakeWatchFactory();
    final refreshStarted = Completer<void>();
    final releaseRefresh = Completer<void>();
    final watcher = GitWorktreeMetadataWatcher(
      repoPath: repo.path,
      onRefresh: () async {
        if (!refreshStarted.isCompleted) {
          refreshStarted.complete();
        }
        await releaseRefresh.future;
      },
      debounce: Duration.zero,
      pollInterval: const Duration(hours: 1),
      watchDirectory: watches.call,
    );
    addTearDown(() async {
      if (!releaseRefresh.isCompleted) {
        releaseRefresh.complete();
      }
      await watcher.dispose();
      await watches.dispose();
    });

    watcher.start();
    watches.emit(worktreesDir.path, p.join(worktreesDir.path, 'external'));
    await refreshStarted.future;

    var suspensionCompleted = false;
    final suspension = watcher.suspendRefresh().then((_) {
      suspensionCompleted = true;
    });
    await Future<void>.delayed(Duration.zero);
    expect(suspensionCompleted, isFalse);

    releaseRefresh.complete();
    await suspension;
    expect(suspensionCompleted, isTrue);
    watcher.resumeRefresh();
  });

  test(
    'low-frequency poll recovers missed events and stops on dispose',
    () async {
      final repo = Directory(p.join(tempDir.path, 'repo'))..createSync();
      Directory(p.join(repo.path, '.git')).createSync();
      final watches = _FakeWatchFactory();
      var refreshes = 0;
      final watcher = GitWorktreeMetadataWatcher(
        repoPath: repo.path,
        onRefresh: () async => refreshes += 1,
        debounce: Duration.zero,
        pollInterval: const Duration(milliseconds: 12),
        watchDirectory: watches.call,
      );

      watcher.start();
      await Future<void>.delayed(const Duration(milliseconds: 45));
      expect(refreshes, greaterThanOrEqualTo(2));

      await watcher.dispose();
      final afterDispose = refreshes;
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(refreshes, afterDispose);
      await watches.dispose();
    },
  );
}

String _absolute(String path) => p.normalize(p.absolute(path));

final class _FakeWatchFactory {
  final Map<String, StreamController<String>> _controllers =
      <String, StreamController<String>>{};
  final List<({String path, bool recursive})> requests =
      <({String path, bool recursive})>[];

  Stream<String> call(String path, {required bool recursive}) {
    final normalized = _absolute(path);
    requests.add((path: normalized, recursive: recursive));
    return _controllers
        .putIfAbsent(
          normalized,
          () => StreamController<String>.broadcast(sync: true),
        )
        .stream;
  }

  bool isWatching(String path, {required bool recursive}) {
    final normalized = _absolute(path);
    return requests.any(
      (request) =>
          request.recursive == recursive && p.equals(request.path, normalized),
    );
  }

  void emit(String watchedPath, String eventPath) {
    _controllers[_absolute(watchedPath)]?.add(_absolute(eventPath));
  }

  Future<void> dispose() async {
    await Future.wait<void>(
      _controllers.values.map((controller) => controller.close()),
    );
  }
}
