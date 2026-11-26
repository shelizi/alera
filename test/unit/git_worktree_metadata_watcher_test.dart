import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/workbench/application/git_worktree_metadata_watcher.dart';
import 'package:alera/src/features/workbench/application/workbench_background_task_pool.dart';
import 'package:fake_async/fake_async.dart';
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

  test('resolveGitCommonDir handles main and linked worktrees', () async {
    final mainRepo = Directory(p.join(tempDir.path, 'main'))..createSync();
    final mainGit = Directory(p.join(mainRepo.path, '.git'))..createSync();
    expect(await resolveGitCommonDir(mainRepo.path), _absolute(mainGit.path));

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

    expect(
      await resolveGitCommonDir(linkedRepo.path),
      _absolute(commonGit.path),
    );
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

      await watcher.start();
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

  test('index churn does not trigger catalog reconciliation', () async {
    final repo = Directory(p.join(tempDir.path, 'repo'))..createSync();
    final gitDir = Directory(p.join(repo.path, '.git'))..createSync();
    final worktreesDir = Directory(p.join(gitDir.path, 'worktrees'))
      ..createSync();
    final watches = _FakeWatchFactory();
    var refreshes = 0;
    final watcher = GitWorktreeMetadataWatcher(
      repoPath: repo.path,
      onRefresh: () async => refreshes += 1,
      debounce: Duration.zero,
      pollInterval: Duration.zero,
      watchDirectory: watches.call,
    );
    addTearDown(() async {
      await watcher.dispose();
      await watches.dispose();
    });
    await watcher.start();

    for (var i = 0; i < 200; i++) {
      watches.emit(
        worktreesDir.path,
        p.join(worktreesDir.path, 'linked-${i % 10}', 'index'),
      );
      watches.emit(
        worktreesDir.path,
        p.join(worktreesDir.path, 'linked-${i % 10}', 'index.lock'),
      );
    }
    await Future<void>.delayed(Duration.zero);
    expect(refreshes, 0);

    // Structural and HEAD events must still be observed, including a
    // linked-worktree administrative directory literally named index.
    for (final parts in [
      ['index'],
      ['linked', 'HEAD'],
      ['linked', 'gitdir'],
      ['linked', 'locked'],
      ['linked', 'unknown', 'index'],
    ]) {
      watches.emit(worktreesDir.path, p.joinAll([worktreesDir.path, ...parts]));
      await Future<void>.delayed(Duration.zero);
    }
    expect(refreshes, 5);
  });

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

      await watcher.start();
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

    await watcher.start();
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

      await watcher.start();
      await Future<void>.delayed(const Duration(milliseconds: 45));
      expect(refreshes, greaterThanOrEqualTo(2));

      await watcher.dispose();
      final afterDispose = refreshes;
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(refreshes, afterDispose);
      await watches.dispose();
    },
  );

  test('metadata probe bursts are serialized and coalesced', () async {
    final repo = Directory(p.join(tempDir.path, 'repo'))..createSync();
    final gitDir = Directory(p.join(repo.path, '.git'))..createSync();
    final worktrees = Directory(p.join(gitDir.path, 'worktrees'))..createSync();
    final watches = _FakeWatchFactory();
    final entered = Completer<void>();
    final release = Completer<void>();
    final trailing = Completer<void>();
    var resolutions = 0;
    var activeProbes = 0;
    var maxActive = 0;
    final watcher = GitWorktreeMetadataWatcher(
      repoPath: repo.path,
      onRefresh: () async {},
      debounce: Duration.zero,
      pollInterval: Duration.zero,
      watchDirectory: watches.call,
      resolveCommonDir: (_) async {
        resolutions += 1;
        activeProbes += 1;
        if (activeProbes > maxActive) maxActive = activeProbes;
        if (resolutions == 2) {
          entered.complete();
          await release.future;
        }
        if (resolutions == 3) trailing.complete();
        activeProbes -= 1;
        return _absolute(gitDir.path);
      },
      directoryExists: (_) async => true,
    );
    addTearDown(() async {
      if (!release.isCompleted) release.complete();
      await watcher.dispose();
      await watches.dispose();
    });
    await watcher.start();
    watches.emit(gitDir.path, worktrees.path);
    await entered.future;
    for (var i = 0; i < 400; i++) {
      watches.emit(gitDir.path, worktrees.path);
    }
    expect(resolutions, 2);
    release.complete();
    await trailing.future.timeout(const Duration(seconds: 3));
    await watcher.dispose();
    expect(resolutions, 3);
    expect(maxActive, 1);
  });

  test(
    '100 project watchers share bounded FIFO slots and fold queued events',
    () async {
      final watches = _FakeWatchFactory();
      final refreshPool = WorkbenchBackgroundTaskPool(maxConcurrent: 4);
      final probePool = WorkbenchBackgroundTaskPool(maxConcurrent: 4);
      final releaseProbes = Completer<void>();
      final releaseRefreshes = Completer<void>();
      final probeOrder = <int>[];
      final refreshOrder = <int>[];
      var activeProbes = 0;
      var activeRefreshes = 0;
      var maxProbes = 0;
      var maxRefreshes = 0;
      var finished = 0;
      final watchers = <GitWorktreeMetadataWatcher>[];
      for (var i = 0; i < 100; i++) {
        final root = p.join(tempDir.path, 'project-$i');
        final git = p.join(root, '.git');
        watchers.add(
          GitWorktreeMetadataWatcher(
            repoPath: root,
            refreshPool: refreshPool,
            metadataProbePool: probePool,
            debounce: Duration.zero,
            pollInterval: Duration.zero,
            watchDirectory: watches.call,
            resolveCommonDir: (_) async {
              probeOrder.add(i);
              activeProbes += 1;
              if (activeProbes > maxProbes) maxProbes = activeProbes;
              await releaseProbes.future;
              activeProbes -= 1;
              return git;
            },
            directoryExists: (_) async => true,
            onRefresh: () async {
              refreshOrder.add(i);
              activeRefreshes += 1;
              if (activeRefreshes > maxRefreshes)
                maxRefreshes = activeRefreshes;
              await releaseRefreshes.future;
              activeRefreshes -= 1;
              finished += 1;
            },
          ),
        );
      }
      addTearDown(() async {
        if (!releaseProbes.isCompleted) releaseProbes.complete();
        if (!releaseRefreshes.isCompleted) releaseRefreshes.complete();
        await Future.wait(watchers.map((watcher) => watcher.dispose()));
        await watches.dispose();
      });
      final startup = Future.wait(watchers.map((watcher) => watcher.start()));
      await _eventually(() => probeOrder.length == 4);
      expect(probeOrder, [0, 1, 2, 3]);
      releaseProbes.complete();
      await startup;
      expect(probeOrder, List.generate(100, (i) => i));
      expect(maxProbes, 4);
      for (final watcher in watchers) {
        final dir = p.join(watcher.repoPath, '.git', 'worktrees');
        watches.emit(dir, p.join(dir, 'external', 'HEAD'));
      }
      await _eventually(() => refreshOrder.length == 4);
      expect(refreshOrder, [0, 1, 2, 3]);
      for (final watcher in watchers.skip(4)) {
        final dir = p.join(watcher.repoPath, '.git', 'worktrees');
        for (var i = 0; i < 10; i++) {
          watches.emit(dir, p.join(dir, 'external', 'HEAD'));
        }
      }
      releaseRefreshes.complete();
      await _eventually(() => finished == 100);
      await Future.wait(watchers.map((watcher) => watcher.dispose()));
      expect(refreshOrder, List.generate(100, (i) => i));
      expect(maxRefreshes, 4);
    },
  );

  test('background pool releases failed slots and cancels only matching pending owners', () async {
    final pool = WorkbenchBackgroundTaskPool(maxConcurrent: 1);
    final release = Completer<void>();
    final order = <String>[];
    final cancelledOwner = Object();
    final failed = pool.run(Object(), () async {
      order.add('failed');
      await release.future;
      throw StateError('failed task');
    });
    final observedFailure = expectLater(failed, throwsStateError);
    final cancelled = pool.run(cancelledOwner, () async {
      order.add('cancelled');
    });
    final following = pool.run(Object(), () async {
      order.add('following');
    });
    expect(pool.cancelPending(cancelledOwner), isTrue);
    expect(pool.cancelPending(cancelledOwner), isFalse);
    await cancelled;
    expect(order, ['failed']);
    release.complete();
    await observedFailure;
    await following;
    expect(order, ['failed', 'following']);
  });

  test('queued refresh suspension does not wait for another project', () async {
    final watches = _FakeWatchFactory();
    final pool = WorkbenchBackgroundTaskPool(maxConcurrent: 1);
    final release = Completer<void>();
    final blocker = pool.run(Object(), () => release.future);
    var refreshes = 0;
    final git = p.join(tempDir.path, '.git');
    final dir = p.join(git, 'worktrees');
    final watcher = GitWorktreeMetadataWatcher(
      repoPath: tempDir.path,
      refreshPool: pool,
      debounce: Duration.zero,
      pollInterval: Duration.zero,
      watchDirectory: watches.call,
      resolveCommonDir: (_) async => git,
      directoryExists: (_) async => true,
      onRefresh: () async => refreshes += 1,
    );
    addTearDown(() async {
      if (!release.isCompleted) release.complete();
      await blocker;
      await watcher.dispose();
      await watches.dispose();
    });
    await watcher.start();
    watches.emit(dir, p.join(dir, 'external', 'HEAD'));
    await Future<void>.delayed(Duration.zero);
    await watcher.suspendRefresh().timeout(const Duration(seconds: 1));
    expect(refreshes, 0);
    watcher.resumeRefresh();
    release.complete();
    await blocker;
    await _eventually(() => refreshes == 1);
    await watcher.dispose();
    expect(refreshes, 1);
  });

  test('disposing a queued probe or refresh releases pending work', () async {
    final pool = WorkbenchBackgroundTaskPool(maxConcurrent: 1);
    final release = Completer<void>();
    final blocker = pool.run(Object(), () => release.future);
    final watches = _FakeWatchFactory();
    var probes = 0;
    var refreshes = 0;
    final git = p.join(tempDir.path, '.git');
    final dir = p.join(git, 'worktrees');
    final first = GitWorktreeMetadataWatcher(
      repoPath: tempDir.path,
      metadataProbePool: pool,
      pollInterval: Duration.zero,
      watchDirectory: watches.call,
      resolveCommonDir: (_) async {
        probes += 1;
        return git;
      },
      directoryExists: (_) async => true,
      onRefresh: () async {},
    );
    final second = GitWorktreeMetadataWatcher(
      repoPath: tempDir.path,
      refreshPool: pool,
      debounce: Duration.zero,
      pollInterval: Duration.zero,
      watchDirectory: watches.call,
      resolveCommonDir: (_) async => git,
      directoryExists: (_) async => true,
      onRefresh: () async => refreshes += 1,
    );
    addTearDown(() async {
      if (!release.isCompleted) release.complete();
      await blocker;
      await first.dispose();
      await second.dispose();
      await watches.dispose();
    });
    final startup = first.start();
    await second.start();
    watches.emit(dir, p.join(dir, 'external', 'HEAD'));
    await Future<void>.delayed(Duration.zero);
    await Future.wait([first.dispose(), second.dispose()])
        .timeout(const Duration(seconds: 1));
    await startup;
    release.complete();
    await blocker;
    expect(probes, 0);
    expect(refreshes, 0);
  });

  test('continuous events reach the maximum debounce deadline', () async {
    late FakeAsync clock;
    late Future<void> cleanup;
    fakeAsync((fakeClock) {
      clock = fakeClock;
      final watches = _FakeWatchFactory();
      var refreshes = 0;
      final git = p.join(tempDir.path, '.git');
      final dir = p.join(git, 'worktrees');
      final watcher = GitWorktreeMetadataWatcher(
        repoPath: tempDir.path,
        debounce: const Duration(milliseconds: 100),
        maxRefreshDelay: const Duration(milliseconds: 250),
        pollInterval: Duration.zero,
        watchDirectory: watches.call,
        resolveCommonDir: (_) async => git,
        directoryExists: (_) async => true,
        onRefresh: () async => refreshes += 1,
      );
      var started = false;
      unawaited(watcher.start().then((_) => started = true));
      clock.flushMicrotasks();
      expect(started, isTrue);
      for (var i = 0; i < 12; i++) {
        watches.emit(dir, p.join(dir, 'external', 'HEAD'));
        clock.elapse(const Duration(milliseconds: 50));
        if (i == 4) expect(refreshes, 1);
        if (i == 9) expect(refreshes, 2);
      }
      clock.elapse(const Duration(milliseconds: 100));
      expect(refreshes, 3);
      cleanup = watcher.dispose().then((_) => watches.dispose());
      clock.flushMicrotasks();
      expect(clock.pendingTimers, isEmpty);
    });
    // Stream cancellation may use an already-resolved Future from the
    // outer zone. Let that zone run before flushing the virtual microtasks.
    var disposed = false;
    unawaited(cleanup.then((_) => disposed = true));
    for (var i = 0; i < 20 && !disposed; i++) {
      await Future<void>.delayed(Duration.zero);
      clock.flushMicrotasks();
    }
    expect(disposed, isTrue);
  });

  test('dispose during initial probe never attaches a late watch', () async {
    final watches = _FakeWatchFactory();
    final pending = Completer<String?>();
    final watcher = GitWorktreeMetadataWatcher(
      repoPath: tempDir.path,
      onRefresh: () async {},
      pollInterval: Duration.zero,
      watchDirectory: watches.call,
      resolveCommonDir: (_) => pending.future,
      directoryExists: (_) async => true,
    );
    final startup = watcher.start();
    final disposal = watcher.dispose();
    pending.complete(_absolute(tempDir.path));
    await startup;
    await disposal;
    expect(watches.requests, isEmpty);
    await watches.dispose();
  });

  test('poll refresh continues while metadata I/O is pending', () async {
    final watches = _FakeWatchFactory();
    final pending = Completer<String?>();
    final refreshed = Completer<void>();
    final watcher = GitWorktreeMetadataWatcher(
      repoPath: tempDir.path,
      onRefresh: () async {
        if (!refreshed.isCompleted) refreshed.complete();
      },
      debounce: Duration.zero,
      pollInterval: const Duration(milliseconds: 10),
      watchDirectory: watches.call,
      resolveCommonDir: (_) => pending.future,
    );
    addTearDown(() async {
      if (!pending.isCompleted) pending.complete(null);
      await watcher.dispose();
      await watches.dispose();
    });
    final startup = watcher.start();
    await refreshed.future.timeout(const Duration(seconds: 3));
    expect(watches.requests, isEmpty);
    pending.complete(null);
    await startup;
  });

  test('worktrees watch recovers after a directory is recreated', () async {
    final repo = Directory(p.join(tempDir.path, 'repo'))..createSync();
    final gitDir = Directory(p.join(repo.path, '.git'))..createSync();
    final worktrees = Directory(p.join(gitDir.path, 'worktrees'))..createSync();
    final watches = _FakeWatchFactory();
    var refreshes = 0;
    final watcher = GitWorktreeMetadataWatcher(
      repoPath: repo.path,
      onRefresh: () async => refreshes += 1,
      debounce: Duration.zero,
      pollInterval: Duration.zero,
      watchDirectory: watches.call,
    );
    addTearDown(() async {
      await watcher.dispose();
      await watches.dispose();
    });
    await watcher.start();
    expect(watches.isActive(worktrees.path), isTrue);

    worktrees.deleteSync();
    watches.emit(gitDir.path, worktrees.path);
    await _eventually(() => !watches.isActive(worktrees.path));

    worktrees.createSync();
    watches.emit(gitDir.path, worktrees.path);
    await _eventually(() => watches.isActive(worktrees.path));
    await Future<void>.delayed(Duration.zero);
    final before = refreshes;
    watches.emit(worktrees.path, p.join(worktrees.path, 'external', 'HEAD'));
    await _eventually(() => refreshes > before);
  });
}

Future<void> _eventually(bool Function() condition) async {
  final deadline = DateTime.now().add(const Duration(seconds: 3));
  while (!condition()) {
    if (DateTime.now().isAfter(deadline)) {
      fail('Timed out waiting for watcher state');
    }
    await Future<void>.delayed(const Duration(milliseconds: 5));
  }
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

  bool isActive(String path) =>
      _controllers[_absolute(path)]?.hasListener ?? false;

  void emit(String watchedPath, String eventPath) {
    _controllers[_absolute(watchedPath)]?.add(_absolute(eventPath));
  }

  Future<void> dispose() async {
    await Future.wait<void>(
      _controllers.values.map((controller) => controller.close()),
    );
  }
}
