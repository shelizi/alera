import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_worktree_metadata_watcher_registry.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('starts one metadata watcher for an unchanged git project', () {
    final fakes = <_FakeWatcher>[];
    final registry = _registry(fakes);
    final project = _project(id: 'project', repoPath: 'C:/repo');

    registry.sync(project, onRefresh: (_) async {});
    registry.sync(project, onRefresh: (_) async {});

    expect(fakes, hasLength(1));
    expect(fakes.single.startCalls, 1);
    expect(fakes.single.disposeCalls, 0);
  });

  test('replaces the watcher when the repository path changes', () {
    final fakes = <_FakeWatcher>[];
    final registry = _registry(fakes);

    registry.sync(
      _project(id: 'project', repoPath: 'C:/repo-a'),
      onRefresh: (_) async {},
    );
    registry.sync(
      _project(id: 'project', repoPath: 'C:/repo-b'),
      onRefresh: (_) async {},
    );

    expect(fakes, hasLength(2));
    expect(fakes.first.disposeCalls, 1);
    expect(fakes.last.startCalls, 1);
  });

  test('folder projects remove metadata watchers', () {
    final fakes = <_FakeWatcher>[];
    final registry = _registry(fakes);

    registry.sync(
      _project(id: 'project', repoPath: 'C:/repo'),
      onRefresh: (_) async {},
    );
    registry.sync(
      _project(id: 'project', repoPath: 'C:/repo', kind: .folder),
      onRefresh: (_) async {},
    );

    expect(fakes.single.disposeCalls, 1);
  });

  test('prune disposes watchers for projects that are no longer live', () {
    final fakes = <_FakeWatcher>[];
    final registry = _registry(fakes);

    registry.sync(
      _project(id: 'keep', repoPath: 'C:/keep'),
      onRefresh: (_) async {},
    );
    registry.sync(
      _project(id: 'remove', repoPath: 'C:/remove'),
      onRefresh: (_) async {},
    );
    registry.prune(const <String>{'keep'});

    expect(fakes[0].disposeCalls, 0);
    expect(fakes[1].disposeCalls, 1);
  });

  test('refresh suspension resumes even when the action fails', () async {
    final fakes = <_FakeWatcher>[];
    final registry = _registry(fakes);
    registry.sync(
      _project(id: 'project', repoPath: 'C:/repo'),
      onRefresh: (_) async {},
    );

    await expectLater(
      registry.withRefreshSuspended<void>('project', () async {
        throw StateError('failed mutation');
      }),
      throwsStateError,
    );

    expect(fakes.single.suspendCalls, 1);
    expect(fakes.single.resumeCalls, 1);
  });

  test('watcher refresh callback preserves its project id', () async {
    final fakes = <_FakeWatcher>[];
    final refreshedProjectIds = <String>[];
    final registry = _registry(fakes);
    registry.sync(
      _project(id: 'project', repoPath: 'C:/repo'),
      onRefresh: (projectId) async => refreshedProjectIds.add(projectId),
    );

    await fakes.single.onRefresh();

    expect(refreshedProjectIds, <String>['project']);
  });
}

WorkbenchWorktreeMetadataWatcherRegistry _registry(List<_FakeWatcher> fakes) {
  return WorkbenchWorktreeMetadataWatcherRegistry(
    factory: ({required repoPath, required onRefresh}) {
      final watcher = _FakeWatcher(repoPath: repoPath, onRefresh: onRefresh);
      fakes.add(watcher);
      return watcher;
    },
  );
}

Project _project({
  required String id,
  required String repoPath,
  ProjectKind kind = ProjectKind.gitRepository,
}) {
  final now = DateTime.utc(2026, 9, 10);
  return Project(
    id: id,
    name: id,
    repoPath: repoPath,
    createdAt: now,
    updatedAt: now,
    kind: kind,
  );
}

final class _FakeWatcher implements WorkbenchWorktreeMetadataWatcherHandle {
  _FakeWatcher({required this.repoPath, required this.onRefresh});

  @override
  final String repoPath;
  final Future<void> Function() onRefresh;
  int startCalls = 0;
  int suspendCalls = 0;
  int resumeCalls = 0;
  int disposeCalls = 0;

  @override
  void start() => startCalls += 1;

  @override
  Future<void> suspendRefresh() async {
    suspendCalls += 1;
  }

  @override
  void resumeRefresh() => resumeCalls += 1;

  @override
  Future<void> dispose() async {
    disposeCalls += 1;
  }
}
