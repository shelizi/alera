import 'dart:async';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_git_auto_fetch_scheduler.dart';
import 'package:test/test.dart';
import 'package:path/path.dart' as p;

final class _ManualTimer implements Timer {
  _ManualTimer(this.callback);

  final void Function() callback;
  bool _active = true;
  int _tick = 0;

  void fire() {
    if (!_active) return;
    _active = false;
    _tick++;
    callback();
  }

  @override
  bool get isActive => _active;

  @override
  int get tick => _tick;

  @override
  void cancel() => _active = false;
}

final class _TimerRecorder {
  final List<Duration> delays = <Duration>[];
  final List<_ManualTimer> timers = <_ManualTimer>[];

  Timer schedule(Duration duration, void Function() callback) {
    delays.add(duration);
    final timer = _ManualTimer(callback);
    timers.add(timer);
    return timer;
  }

  _ManualTimer get latest => timers.last;
}

Project _project(
  String id,
  String repoPath, {
  ProjectKind kind = ProjectKind.gitRepository,
}) => Project(
  id: id,
  name: id,
  repoPath: repoPath,
  createdAt: DateTime.utc(2026),
  updatedAt: DateTime.utc(2026),
  kind: kind,
);

void main() {
  test('fetch targets are project repositories, not linked worktrees', () {
    final paths = gitAutoFetchRepositoryPaths(<Project>[
      _project('git-1', '/projects/one'),
      _project('git-2', '/projects/one'),
      _project('folder', '/projects/folder', kind: ProjectKind.folder),
      _project('git-3', '/projects/two'),
      _project('empty', '   '),
    ]);
    expect(paths, <String>[
      p.normalize('/projects/one'),
      p.normalize('/projects/two'),
    ]);
  });

  test(
    'enabled scheduler fetches once per period and can be disabled',
    () async {
      final timer = _TimerRecorder();
      final fetched = <String>[];
      final scheduler = GitAutoFetchScheduler(
        fetch: (path) async => fetched.add(path),
        onFetched: (_) {},
        onError: (_, _, _) => fail('unexpected error'),
        createTimer: timer.schedule,
      );
      addTearDown(scheduler.dispose);

      scheduler.configure(enabled: true, paths: <String>['/one', '/two']);
      expect(timer.delays, <Duration>[workbenchGitAutoFetchStartupDelay]);
      scheduler.configure(enabled: true, paths: <String>['/one', '/two']);
      expect(
        timer.delays.length,
        1,
        reason: 'same configuration must not reset timer',
      );

      await scheduler.fetchNow();
      expect(fetched, <String>['/one', '/two']);
      expect(timer.delays.last, workbenchGitAutoFetchInterval);

      scheduler.configure(enabled: false, paths: <String>['/one', '/two']);
      expect(timer.latest.isActive, isFalse);
      await scheduler.fetchNow();
      expect(fetched, <String>['/one', '/two']);

      scheduler.configure(enabled: true, paths: <String>['/one']);
      expect(timer.delays.last, workbenchGitAutoFetchStartupDelay);
      await scheduler.fetchNow();
      expect(fetched.last, '/one');
    },
  );

  test('timer callback launches a fetch and schedules next interval', () async {
    final timer = _TimerRecorder();
    final fetched = <String>[];
    final scheduler = GitAutoFetchScheduler(
      fetch: (path) async => fetched.add(path),
      onFetched: (_) {},
      onError: (_, _, _) => fail('unexpected error'),
      createTimer: timer.schedule,
    );
    addTearDown(scheduler.dispose);

    scheduler.configure(enabled: true, paths: <String>['/project']);
    timer.latest.fire();
    await Future<void>.delayed(Duration.zero);
    expect(fetched, <String>['/project']);
    expect(timer.delays.last, workbenchGitAutoFetchInterval);
  });

  test(
    'fetch is serialized and disabling stops remaining repositories',
    () async {
      final timer = _TimerRecorder();
      final gate = Completer<void>();
      final attempted = <String>[];
      final scheduler = GitAutoFetchScheduler(
        fetch: (path) async {
          attempted.add(path);
          if (path == '/one') await gate.future;
        },
        onFetched: (_) {},
        onError: (_, _, _) => fail('unexpected error'),
        createTimer: timer.schedule,
      );
      addTearDown(scheduler.dispose);

      scheduler.configure(enabled: true, paths: <String>['/one', '/two']);
      final running = scheduler.fetchNow();
      await scheduler.fetchNow();
      expect(attempted, <String>['/one']);
      scheduler.configure(enabled: false, paths: <String>['/one', '/two']);
      gate.complete();
      await running;
      expect(attempted, <String>['/one']);
      expect(timer.latest.isActive, isFalse);
    },
  );

  test(
    'errors are isolated, logged, and retried with bounded backoff',
    () async {
      final timer = _TimerRecorder();
      final fetched = <String>[];
      final errors = <String>[];
      var failFirst = true;
      final scheduler = GitAutoFetchScheduler(
        fetch: (path) async {
          fetched.add(path);
          if (failFirst && path == '/broken') throw StateError('offline');
        },
        onFetched: (_) {},
        onError: (path, _, _) => errors.add(path),
        createTimer: timer.schedule,
      );
      addTearDown(scheduler.dispose);

      scheduler.configure(
        enabled: true,
        paths: <String>['/broken', '/healthy'],
      );
      await scheduler.fetchNow();
      expect(fetched, <String>['/broken', '/healthy']);
      expect(errors, <String>['/broken']);
      expect(timer.delays.last, const Duration(minutes: 10));

      failFirst = false;
      await scheduler.fetchNow();
      expect(timer.delays.last, workbenchGitAutoFetchInterval);
    },
  );

  test('changing targets during fetch queues new configuration', () async {
    final timer = _TimerRecorder();
    final gate = Completer<void>();
    final attempted = <String>[];
    final scheduler = GitAutoFetchScheduler(
      fetch: (path) async {
        attempted.add(path);
        if (path == '/old') await gate.future;
      },
      onFetched: (_) {},
      onError: (_, _, _) => fail('unexpected error'),
      createTimer: timer.schedule,
    );
    addTearDown(scheduler.dispose);

    scheduler.configure(enabled: true, paths: <String>['/old', '/removed']);
    final running = scheduler.fetchNow();
    scheduler.configure(enabled: true, paths: <String>['/old', '/new']);
    gate.complete();
    await running;
    expect(attempted, <String>['/old']);
    expect(timer.delays.last, workbenchGitAutoFetchStartupDelay);
    await scheduler.fetchNow();
    expect(attempted, <String>['/old', '/old', '/new']);
  });

  test('dispose cancels timers and avoids post-dispose updates', () async {
    final timer = _TimerRecorder();
    final gate = Completer<void>();
    var updated = false;
    final scheduler = GitAutoFetchScheduler(
      fetch: (_) => gate.future,
      onFetched: (_) => updated = true,
      onError: (_, _, _) => fail('unexpected error'),
      createTimer: timer.schedule,
    );
    scheduler.configure(enabled: true, paths: <String>['/one']);
    final pending = scheduler.fetchNow();
    scheduler.dispose();
    gate.complete();
    await pending;
    expect(updated, isFalse);
    expect(timer.latest.isActive, isFalse);
  });
}
