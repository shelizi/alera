import 'dart:async';

import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';

import 'package:alera/src/features/workbench/application/workspace_activity_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_activity_repository.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class _FakeWorkspaceActivityRepository implements WorkspaceActivityRepository {
  final Map<String, DateTime> stored = <String, DateTime>{};
  int upsertCalls = 0;
  Completer<void>? upsertStarted;
  Completer<void>? upsertRelease;
  Completer<void>? upsertCompleted;

  @override
  Future<Map<String, DateTime>> loadAll() async =>
      Map<String, DateTime>.from(stored);

  @override
  Future<void> upsertAll(Map<String, DateTime> entries) async {
    upsertCalls++;
    final started = upsertStarted;
    final release = upsertRelease;
    final completed = upsertCompleted;
    if (started != null) {
      upsertStarted = null;
      upsertRelease = null;
      upsertCompleted = null;
      if (!started.isCompleted) {
        started.complete();
      }
      if (release != null) {
        await release.future;
      }
    }
    stored.addAll(entries);
    if (completed != null && !completed.isCompleted) {
      completed.complete();
    }
  }

  @override
  Future<void> remove(String workspaceId) async {
    stored.remove(workspaceId);
  }
}

void main() {
  test('recordActivity updates in-memory state immediately', () {
    final container = ProviderContainer();
    addTearDown(container.dispose);
    final controller = container.read(
      workspaceActivityControllerProvider.notifier,
    );
    final at = DateTime.utc(2026, 7, 4, 12);

    controller.recordActivity('w-1', at);

    expect(container.read(workspaceActivityControllerProvider), {'w-1': at});
  });

  test('older activity does not overwrite a newer timestamp', () {
    final container = ProviderContainer();
    addTearDown(container.dispose);
    final controller = container.read(
      workspaceActivityControllerProvider.notifier,
    );
    final newer = DateTime.utc(2026, 7, 4, 12);
    final older = DateTime.utc(2026, 7, 4, 11);

    controller.recordActivity('w-1', newer);
    controller.recordActivity('w-1', older);

    expect(container.read(workspaceActivityControllerProvider), {'w-1': newer});
  });

  test(
    'attachRepository seeds persisted entries under newer in-memory ones',
    () async {
      final container = ProviderContainer();
      addTearDown(container.dispose);
      final repository = _FakeWorkspaceActivityRepository()
        ..stored['w-persisted'] = DateTime.utc(2026, 7, 1)
        ..stored['w-live'] = DateTime.utc(2026, 7, 1);
      final controller = container.read(
        workspaceActivityControllerProvider.notifier,
      );
      final live = DateTime.utc(2026, 7, 4);
      controller.recordActivity('w-live', live);

      await controller.attachRepository(repository);

      final state = container.read(workspaceActivityControllerProvider);
      expect(state['w-persisted'], DateTime.utc(2026, 7, 1));
      expect(state['w-live'], live);
    },
  );

  test(
    'agent transition bursts publish one activity map with latest timestamps',
    () async {
      final container = ProviderContainer();
      addTearDown(container.dispose);
      final controller = container.read(
        workspaceActivityControllerProvider.notifier,
      );
      final repository = _FakeWorkspaceActivityRepository();
      await controller.attachRepository(repository);
      container.read(workspaceActivityCoordinatorProvider);
      final agent = container.read(agentStatusControllerProvider.notifier);
      final older = DateTime.utc(2026, 7, 4, 11);
      final newer = DateTime.utc(2026, 7, 4, 12);
      AgentStatusEntry entry(String id, String workspace, DateTime at) =>
          AgentStatusEntry(
            terminalSessionId: id,
            workspaceId: workspace,
            tabId: id,
            agentType: AgentType.codex,
            state: AgentStatusState.working,
            prompt: 'Work',
            updatedAt: at,
            stateStartedAt: at,
          );
      final entries = [
        entry('a', 'w-1', newer),
        entry('b', 'w-1', older),
        for (var i = 2; i <= 100; i++) entry('session-$i', 'w-$i', newer),
      ];
      final initial = container.read(workspaceActivityControllerProvider);
      var updates = 0;
      final sub = container.listen(workspaceActivityControllerProvider, (_, _) {
        updates++;
      });
      addTearDown(sub.close);
      agent.mergeRuntimeDelta(upserts: entries);
      final first = container.read(workspaceActivityControllerProvider);
      expect(initial, isEmpty);
      expect(first, hasLength(100));
      expect(first['w-1'], newer);
      expect(updates, 1);

      // Updated tool metadata is not a new workspace activity.
      agent.mergeRuntimeDelta(
        upserts: [
          for (final value in entries)
            value.copyWith(
              toolName: 'Bash',
              updatedAt: newer.add(const Duration(minutes: 1)),
            ),
        ],
      );
      expect(container.read(workspaceActivityControllerProvider), same(first));
      expect(updates, 1);
      // A newer timestamp for one workspace and a stale one for another:
      // publish once and do not change the old map.
      final later = newer.add(const Duration(minutes: 2));
      agent.mergeRuntimeDelta(
        upserts: [
          entries[0].copyWith(
            state: AgentStatusState.waiting,
            stateStartedAt: later,
            updatedAt: later,
          ),
          entries[2].copyWith(
            state: AgentStatusState.done,
            stateStartedAt: older,
            updatedAt: later,
          ),
        ],
      );
      final second = container.read(workspaceActivityControllerProvider);
      expect(updates, 2);
      expect(second['w-1'], later);
      expect(second['w-2'], newer);
      expect(first['w-1'], newer);
      await Future.pause(
        workspaceActivityFlushDelay + const Duration(milliseconds: 100),
      );
      expect(repository.upsertCalls, 1);
      expect(repository.stored, second);
    },
  );

  test('recorded activity is flushed to the repository in one batch', () async {
    final container = ProviderContainer();
    addTearDown(container.dispose);
    final repository = _FakeWorkspaceActivityRepository();
    final controller = container.read(
      workspaceActivityControllerProvider.notifier,
    );
    await controller.attachRepository(repository);

    controller.recordActivity('w-1', .utc(2026, 7, 4, 12));
    controller.recordActivity('w-2', .utc(2026, 7, 4, 13));

    await Future.pause(
      workspaceActivityFlushDelay + const Duration(milliseconds: 100),
    );
    expect(repository.upsertCalls, 1);
    expect(repository.stored.keys, containsAll(<String>['w-1', 'w-2']));
  });

  test(
    'removeWorkspace is not undone by an older in-flight activity flush',
    () async {
      final container = ProviderContainer();
      addTearDown(container.dispose);
      final repository = _FakeWorkspaceActivityRepository();
      final controller = container.read(
        workspaceActivityControllerProvider.notifier,
      );
      await controller.attachRepository(repository);
      final started = Completer<void>();
      final release = Completer<void>();
      final completed = Completer<void>();
      repository
        ..upsertStarted = started
        ..upsertRelease = release
        ..upsertCompleted = completed;

      controller.recordActivity('w-1', .utc(2026, 7, 4, 12));
      await started.future;
      controller.removeWorkspace('w-1');
      await Future<void>.delayed(Duration.zero);

      release.complete();
      await completed.future;
      await Future<void>.delayed(Duration.zero);

      expect(container.read(workspaceActivityControllerProvider), isEmpty);
      expect(repository.stored, isEmpty);
    },
  );

  test('removeWorkspace drops the entry from state and persistence', () async {
    final container = ProviderContainer();
    addTearDown(container.dispose);
    final repository = _FakeWorkspaceActivityRepository()
      ..stored['w-1'] = DateTime.utc(2026, 7, 1);
    final controller = container.read(
      workspaceActivityControllerProvider.notifier,
    );
    await controller.attachRepository(repository);

    controller.removeWorkspace('w-1');
    await Future.pause(.zero);

    expect(container.read(workspaceActivityControllerProvider), isEmpty);
    expect(repository.stored, isEmpty);
  });
}
