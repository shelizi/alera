import 'dart:async';

import 'package:alera/src/features/workbench/application/source_control_refresh_pool.dart';
import 'package:alera/src/features/workbench/application/source_control_watcher.dart';
import 'package:alera/src/features/workbench/application/workbench_background_task_pool.dart';
import 'package:alera/src/features/workbench/application/workspace_source_control_controller.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fake_git_backend.dart';

final class _PathWatcher extends SourceControlWatcher {
  final streams = <String, StreamController<native.SourceControlWatchSignal>>{};

  @override
  Future<native.SourceControlWatcherHandle> start({
    required String workspacePath,
  }) async {
    streams[workspacePath] =
        StreamController<native.SourceControlWatchSignal>.broadcast();
    return native.SourceControlWatcherHandle(id: workspacePath);
  }

  @override
  Stream<native.SourceControlWatchSignal> events({
    required native.SourceControlWatcherHandle handle,
  }) => streams[handle.id]!.stream;

  void emit(String path) {
    streams[path]!.add(
      const native.SourceControlWatchSignal(coalescedEventCount: 1),
    );
  }

  @override
  Future<void> stop({required native.SourceControlWatcherHandle handle}) async {
    await streams.remove(handle.id)?.close();
  }

  Future<void> dispose() async {
    final remaining = streams.values.toList();
    streams.clear();
    await Future.wait(remaining.map((stream) => stream.close()));
  }
}

GitStatusResult _status(int version) => GitStatusResult(
  entries: [
    GitChangeEntry(
      path: 'version-$version.txt',
      area: .unstaged,
      status: .modified,
      added: version,
      removed: 0,
    ),
  ],
);

class _MeasuredGitBackend extends FakeGitBackend {
  bool measure = false;
  final versions = <String, int>{};
  final backgroundStarts = <String>[];
  final completed = <String>[];
  final blocked = <String, Completer<void>>{};
  int active = 0;
  int peak = 0;

  @override
  Future<GitStatusResult> status(String path) async {
    calls.add(GitBackendCall('status', <String, Object?>{'path': path}));
    final snapshot = _status(versions[path] ?? 1);
    if (!measure) return snapshot;
    backgroundStarts.add(path);
    active++;
    if (active > peak) peak = active;
    try {
      final gate = blocked[path];
      if (gate != null) await gate.future;
      completed.add(path);
      return snapshot;
    } finally {
      active--;
    }
  }

  void releaseAll() {
    for (final gate in blocked.values) {
      if (!gate.isCompleted) gate.complete();
    }
    blocked.clear();
  }
}

Future<void> _eventually(bool Function() condition) async {
  final timer = Stopwatch()..start();
  while (!condition()) {
    if (timer.elapsed > const Duration(seconds: 10)) {
      fail('Timed out awaiting expected background state');
    }
    await Future.pause(const Duration(milliseconds: 10));
  }
}

void main() {
  test(
    '100 worktrees share four background slots; queued changes coalesce',
    () async {
      const count = 100;
      final backend = _MeasuredGitBackend();
      final watcher = _PathWatcher();
      final container = ProviderContainer(
        overrides: [
          gitBackendProvider.overrideWithValue(backend),
          sourceControlWatcherProvider.overrideWithValue(watcher),
        ],
      );
      addTearDown(() async {
        backend.releaseAll();
        container.dispose();
        await watcher.dispose();
      });
      final paths = [for (var i = 0; i < count; i++) '/repo/worktree-$i'];
      final providers = [
        for (final path in paths)
          workspaceSourceControlControllerProvider(path),
      ];
      final subscriptions = [
        for (final provider in providers) container.listen(provider, (_, _) {}),
      ];
      await Future.wait(
        providers.map((provider) => container.read(provider.future)),
      );
      await _eventually(() => watcher.streams.length == count);
      // Let every already-created watcher attach its listener.
      await Future.pause(const Duration(milliseconds: 20));
      backend.measure = true;
      for (final path in paths) {
        backend.versions[path] = 2;
        backend.blocked[path] = Completer<void>();
        watcher.emit(path);
      }
      await _eventually(() => backend.backgroundStarts.length == 4);
      // All four active scans are held; the other 96 must wait FIFO.
      await Future.pause(const Duration(milliseconds: 250));
      expect(backend.active, 4);
      expect(backend.backgroundStarts, paths.take(4));
      expect(backend.peak, 4);

      // 960 extra events on queued worktrees are covered by the not-yet-started
      // authoritative scans. A change during an active scan needs a replay.
      for (final path in paths.skip(4)) {
        backend.versions[path] = 3;
        for (var i = 0; i < 10; i++) {
          watcher.emit(path);
        }
      }
      backend.versions[paths.first] = 4;
      watcher.emit(paths.first);
      await Future.pause(const Duration(milliseconds: 250));
      expect(backend.backgroundStarts.length, 4);

      // Remove one queued owner, never start its native status call.
      final removed = paths.last;
      subscriptions.last.close();
      await container.pump();
      await _eventually(() => !watcher.streams.containsKey(removed));

      backend.releaseAll();
      await _eventually(
        () =>
            backend.completed.length == count &&
            container
                    .read(providers.first)
                    .requireValue
                    .status
                    .entries
                    .single
                    .added ==
                4 &&
            providers
                .skip(4)
                .take(count - 5)
                .every(
                  (provider) =>
                      container
                          .read(provider)
                          .requireValue
                          .status
                          .entries
                          .single
                          .added ==
                      3,
                ),
      );
      await Future.pause(const Duration(milliseconds: 400));
      expect(backend.peak, 4);
      expect(backend.active, 0);
      expect(backend.backgroundStarts, [...paths.take(count - 1), paths.first]);
      expect(backend.backgroundStarts.contains(removed), isFalse);
      expect(backend.backgroundStarts.length, 100);
      expect(backend.completed.length, 100);
      expect(
        container.read(providers[1]).requireValue.status.entries.single.added,
        2,
      );
    },
  );

  test('manual refresh bypasses a blocked watcher pool; queued scan sees latest state', () async {
    final pool = WorkbenchBackgroundTaskPool(maxConcurrent: 1);
    final gate = Completer<void>();
    final blocker = pool.run(Object(), () => gate.future);
    final backend = _MeasuredGitBackend();
    final watcher = _PathWatcher();
    final container = ProviderContainer(
      overrides: [
        gitBackendProvider.overrideWithValue(backend),
        sourceControlWatcherProvider.overrideWithValue(watcher),
        sourceControlRefreshPoolProvider.overrideWithValue(pool),
      ],
    );
    addTearDown(() async {
      if (!gate.isCompleted) gate.complete();
      await blocker;
      container.dispose();
      await watcher.dispose();
    });
    const path = '/repo/foreground';
    final provider = workspaceSourceControlControllerProvider(path);
    container.listen(provider, (_, _) {});
    await container.read(provider.future);
    await Future.pause(const Duration(milliseconds: 20));
    backend.measure = true;
    backend.versions[path] = 2;
    watcher.emit(path);
    await Future.pause(const Duration(milliseconds: 250));
    expect(backend.backgroundStarts, isEmpty);
    await container
        .read(provider.notifier)
        .refresh()
        .timeout(const Duration(seconds: 3));
    expect(
      container.read(provider).requireValue.status.entries.single.added,
      2,
    );
    expect(backend.backgroundStarts, [path]);
    // A new external edit while the automatic request waits must be visible
    // when it actually obtains its slot, even after a newer manual refresh.
    backend.versions[path] = 3;
    watcher.emit(path);
    gate.complete();
    await blocker;
    await _eventually(
      () =>
          container.read(provider).requireValue.status.entries.single.added ==
          3,
    );
    await Future.pause(const Duration(milliseconds: 250));
    expect(backend.backgroundStarts, [path, path]);
  });

  test(
    'queued watcher defers when a foreground mutation is still busy',
    () async {
      final pool = WorkbenchBackgroundTaskPool(maxConcurrent: 1);
      final blockerGate = Completer<void>();
      final blocker = pool.run(Object(), () => blockerGate.future);
      final mutationGate = Completer<void>();
      final backend = _MutationBackend(mutationGate);
      final watcher = _PathWatcher();
      final container = ProviderContainer(
        overrides: [
          gitBackendProvider.overrideWithValue(backend),
          sourceControlWatcherProvider.overrideWithValue(watcher),
          sourceControlRefreshPoolProvider.overrideWithValue(pool),
        ],
      );
      addTearDown(() async {
        if (!blockerGate.isCompleted) blockerGate.complete();
        if (!mutationGate.isCompleted) mutationGate.complete();
        await blocker;
        container.dispose();
        await watcher.dispose();
      });
      const path = '/repo/busy';
      final provider = workspaceSourceControlControllerProvider(path);
      container.listen(provider, (_, _) {});
      await container.read(provider.future);
      await Future.pause(const Duration(milliseconds: 20));
      backend.measure = true;
      watcher.emit(path);
      await Future.pause(const Duration(milliseconds: 250));
      final mutation = container.read(provider.notifier).fetch();
      expect(container.read(provider).requireValue.isBusy, isTrue);
      blockerGate.complete();
      await blocker;
      await Future.pause(const Duration(milliseconds: 350));
      expect(backend.backgroundStarts, isEmpty);
      expect(container.read(provider).requireValue.isBusy, isTrue);
      backend.versions[path] = 5;
      mutationGate.complete();
      await mutation;
      expect(
        container.read(provider).requireValue.status.entries.single.added,
        5,
      );
      await _eventually(() => backend.backgroundStarts.length == 2);
      expect(container.read(provider).requireValue.isBusy, isFalse);
    },
  );
}

final class _MutationBackend extends _MeasuredGitBackend {
  _MutationBackend(this.gate);
  final Completer<void> gate;

  @override
  Future<void> fetch(String path) => gate.future;
}
