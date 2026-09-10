import 'dart:async';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_main_workspace_preparation_coordinator.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('ensures the main workspace before reconciling the project', () async {
    final coordinator = WorkbenchMainWorkspacePreparationCoordinator();
    final events = <String>[];
    final project = _project('project');

    await coordinator.prepare(
      project: project,
      ensureMainWorkspace: (_) async => events.add('ensure'),
      reconcile: (_) async => events.add('reconcile'),
      onError: (_, _) => events.add('error'),
    );

    expect(events, <String>['ensure', 'reconcile']);
  });

  test('coalesces concurrent preparation for the same project', () async {
    final coordinator = WorkbenchMainWorkspacePreparationCoordinator();
    final gate = Completer<void>();
    var ensureCalls = 0;
    var reconcileCalls = 0;
    final project = _project('project');

    final first = coordinator.prepare(
      project: project,
      ensureMainWorkspace: (_) async {
        ensureCalls += 1;
        await gate.future;
      },
      reconcile: (_) async => reconcileCalls += 1,
      onError: (_, _) {},
    );
    final second = coordinator.prepare(
      project: project,
      ensureMainWorkspace: (_) async => ensureCalls += 1,
      reconcile: (_) async => reconcileCalls += 1,
      onError: (_, _) {},
    );

    await second;
    expect(ensureCalls, 1);
    expect(reconcileCalls, 0);

    gate.complete();
    await first;
    expect(ensureCalls, 1);
    expect(reconcileCalls, 1);
  });

  test('allows different projects to prepare concurrently', () async {
    final coordinator = WorkbenchMainWorkspacePreparationCoordinator();
    final firstGate = Completer<void>();
    final started = <String>[];

    final first = coordinator.prepare(
      project: _project('first'),
      ensureMainWorkspace: (project) async {
        started.add(project.id);
        await firstGate.future;
      },
      reconcile: (_) async {},
      onError: (_, _) {},
    );
    final second = coordinator.prepare(
      project: _project('second'),
      ensureMainWorkspace: (project) async => started.add(project.id),
      reconcile: (_) async {},
      onError: (_, _) {},
    );

    await second;
    expect(started, containsAll(<String>['first', 'second']));

    firstGate.complete();
    await first;
  });

  test('reports failures, releases the project, and allows retry', () async {
    final coordinator = WorkbenchMainWorkspacePreparationCoordinator();
    final errors = <Object>[];
    var ensureCalls = 0;
    final project = _project('project');

    Future<void> prepare() => coordinator.prepare(
      project: project,
      ensureMainWorkspace: (_) async {
        ensureCalls += 1;
        if (ensureCalls == 1) {
          throw StateError('prepare failed');
        }
      },
      reconcile: (_) async {},
      onError: (_, error) => errors.add(error),
    );

    await prepare();
    await prepare();

    expect(ensureCalls, 2);
    expect(errors, hasLength(1));
    expect(errors.single, isA<StateError>());
  });
}

Project _project(String id) {
  final now = DateTime.utc(2026, 9, 10);
  return Project(
    id: id,
    name: id,
    repoPath: 'C:/$id',
    createdAt: now,
    updatedAt: now,
  );
}
