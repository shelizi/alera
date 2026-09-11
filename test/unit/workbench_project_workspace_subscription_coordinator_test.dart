import 'dart:async';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_project_workspace_subscription_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_subscription_registry.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('new live project syncs metadata, watches workspaces, then schedules main workspace', () async {
    final workspaceRegistry = WorkbenchWorkspaceSubscriptionRegistry();
    final tabRegistry = WorkbenchTabSubscriptionRegistry();
    final workspaceStream = StreamController<List<Workspace>>();
    addTearDown(workspaceStream.close);
    final events = <String>[];
    final project = _project('project');

    WorkbenchProjectWorkspaceSubscriptionCoordinator(
      workspaceSubscriptions: workspaceRegistry,
      tabSubscriptions: tabRegistry,
    ).sync(
      projects: <Project>[project],
      validProjectIds: <String>{project.id},
      syncMetadataWatcher: (project) => events.add('metadata:${project.id}'),
      watchWorkspaces: (projectId) {
        events.add('watch:$projectId');
        return workspaceStream.stream;
      },
      onWorkspacesChanged: (project, workspaces) =>
          events.add('data:${project.id}:${workspaces.length}'),
      ensureMainWorkspaceInBackground: (project) =>
          events.add('ensure:${project.id}'),
      forgetClearedLayouts: (workspaceIds) =>
          events.add('forget:${workspaceIds.join(',')}'),
      pruneMetadataWatchers: (projectIds) =>
          events.add('prune:${projectIds.join(',')}'),
    );

    expect(events, <String>[
      'metadata:project',
      'watch:project',
      'ensure:project',
      'prune:project',
    ]);
    expect(workspaceRegistry.contains(project.id), isTrue);

    workspaceStream.add(<Workspace>[_workspace('workspace', project.id)]);
    await Future<void>.delayed(Duration.zero);
    expect(events.last, 'data:project:1');
  });

  test('already-watched project still syncs metadata but does not ensure main again', () {
    final workspaceRegistry = WorkbenchWorkspaceSubscriptionRegistry();
    final tabRegistry = WorkbenchTabSubscriptionRegistry();
    final stream = StreamController<List<Workspace>>();
    addTearDown(stream.close);
    workspaceRegistry.watch(
      projectId: 'project',
      stream: stream.stream,
      onData: (_) {},
    );
    final events = <String>[];
    final project = _project('project');

    WorkbenchProjectWorkspaceSubscriptionCoordinator(
      workspaceSubscriptions: workspaceRegistry,
      tabSubscriptions: tabRegistry,
    ).sync(
      projects: <Project>[project],
      validProjectIds: <String>{project.id},
      syncMetadataWatcher: (project) => events.add('metadata:${project.id}'),
      watchWorkspaces: (_) {
        events.add('watch');
        return const Stream<List<Workspace>>.empty();
      },
      onWorkspacesChanged: (_, _) => events.add('data'),
      ensureMainWorkspaceInBackground: (_) => events.add('ensure'),
      forgetClearedLayouts: (ids) => events.add('forget:${ids.length}'),
      pruneMetadataWatchers: (ids) => events.add('prune:${ids.join(',')}'),
    );

    expect(events, <String>['metadata:project', 'prune:project']);
  });

  test('removed project cancels workspace and nested tab subscriptions before clearing layouts', () {
    final workspaceRegistry = WorkbenchWorkspaceSubscriptionRegistry();
    final tabRegistry = WorkbenchTabSubscriptionRegistry();
    final workspaceStream = StreamController<List<Workspace>>();
    final tabStream = StreamController<List<WorkspaceTabRecord>>();
    addTearDown(workspaceStream.close);
    addTearDown(tabStream.close);
    workspaceRegistry.watch(
      projectId: 'removed',
      stream: workspaceStream.stream,
      onData: (_) {},
    );
    tabRegistry.watch(
      projectId: 'removed',
      workspaceId: 'workspace',
      stream: tabStream.stream,
      onData: (_) {},
    );
    final events = <String>[];

    WorkbenchProjectWorkspaceSubscriptionCoordinator(
      workspaceSubscriptions: workspaceRegistry,
      tabSubscriptions: tabRegistry,
    ).sync(
      projects: const <Project>[],
      validProjectIds: const <String>{},
      syncMetadataWatcher: (_) => events.add('metadata'),
      watchWorkspaces: (_) => const Stream<List<Workspace>>.empty(),
      onWorkspacesChanged: (_, _) {},
      ensureMainWorkspaceInBackground: (_) => events.add('ensure'),
      forgetClearedLayouts: (workspaceIds) {
        events.add(
          'forget:${workspaceIds.join(',')}:${workspaceRegistry.contains('removed')}:${tabRegistry.contains('workspace')}',
        );
      },
      pruneMetadataWatchers: (projectIds) =>
          events.add('prune:${projectIds.length}'),
    );

    expect(events, <String>['forget:workspace:false:false', 'prune:0']);
    expect(workspaceRegistry.contains('removed'), isFalse);
    expect(tabRegistry.contains('workspace'), isFalse);
  });

  test(
    'removed project without nested tabs still clears an empty layout set',
    () {
      final workspaceRegistry = WorkbenchWorkspaceSubscriptionRegistry();
      final tabRegistry = WorkbenchTabSubscriptionRegistry();
      final stream = StreamController<List<Workspace>>();
      addTearDown(stream.close);
      workspaceRegistry.watch(
        projectId: 'removed',
        stream: stream.stream,
        onData: (_) {},
      );
      final forgotten = <List<String>>[];

      WorkbenchProjectWorkspaceSubscriptionCoordinator(
        workspaceSubscriptions: workspaceRegistry,
        tabSubscriptions: tabRegistry,
      ).sync(
        projects: const <Project>[],
        validProjectIds: const <String>{},
        syncMetadataWatcher: (_) {},
        watchWorkspaces: (_) => const Stream<List<Workspace>>.empty(),
        onWorkspacesChanged: (_, _) {},
        ensureMainWorkspaceInBackground: (_) {},
        forgetClearedLayouts: (ids) => forgotten.add(ids.toList()),
        pruneMetadataWatchers: (_) {},
      );

      expect(forgotten, <List<String>>[const <String>[]]);
    },
  );
}

Project _project(String id) {
  final now = DateTime.utc(2026, 9, 11);
  return Project(
    id: id,
    name: id,
    repoPath: 'repo/$id',
    createdAt: now,
    updatedAt: now,
  );
}

Workspace _workspace(String id, String projectId) {
  final now = DateTime.utc(2026, 9, 11);
  return Workspace(
    id: id,
    projectId: projectId,
    name: id,
    path: 'repo/$projectId/$id',
    createdAt: now,
    updatedAt: now,
    kind: WorkspaceKind.linked,
    status: WorkspaceStatus.active,
  );
}
