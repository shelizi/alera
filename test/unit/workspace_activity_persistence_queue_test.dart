import 'dart:async';

import 'package:alera/src/features/workbench/application/workspace_activity_persistence_queue.dart';
import 'package:alera/src/features/workbench/application/workspace_activity_repository.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('a failed write does not block the following removal', () async {
    final repository = _RecordingRepository()..failNextUpsert = true;
    final queue = WorkspaceActivityPersistenceQueue();

    await expectLater(
      queue.upsertAll(
        repository: repository,
        entries: <String, DateTime>{'workspace': DateTime.utc(2026, 9, 11)},
      ),
      throwsStateError,
    );
    await queue.remove(repository: repository, workspaceId: 'workspace');

    expect(repository.events, <String>['upsert', 'remove:workspace']);
  });

  test('a later removal waits for an in-flight write', () async {
    final repository = _RecordingRepository();
    final queue = WorkspaceActivityPersistenceQueue();
    final started = Completer<void>();
    final release = Completer<void>();
    repository
      ..upsertStarted = started
      ..upsertRelease = release;

    final upsert = queue.upsertAll(
      repository: repository,
      entries: <String, DateTime>{'workspace': DateTime.utc(2026, 9, 11)},
    );
    await started.future;
    final removal = queue.remove(
      repository: repository,
      workspaceId: 'workspace',
    );
    await Future<void>.delayed(Duration.zero);

    expect(repository.events, <String>['upsert']);

    release.complete();
    await Future.wait<void>(<Future<void>>[upsert, removal]);
    expect(repository.events, <String>['upsert', 'remove:workspace']);
  });
}

final class _RecordingRepository implements WorkspaceActivityRepository {
  final List<String> events = <String>[];
  bool failNextUpsert = false;
  Completer<void>? upsertStarted;
  Completer<void>? upsertRelease;

  @override
  Future<Map<String, DateTime>> loadAll() async => const <String, DateTime>{};

  @override
  Future<void> upsertAll(Map<String, DateTime> entries) async {
    events.add('upsert');
    final started = upsertStarted;
    if (started != null && !started.isCompleted) {
      started.complete();
    }
    final release = upsertRelease;
    if (release != null) {
      await release.future;
    }
    if (failNextUpsert) {
      failNextUpsert = false;
      throw StateError('write failed');
    }
  }

  @override
  Future<void> remove(String workspaceId) async {
    events.add('remove:$workspaceId');
  }
}
