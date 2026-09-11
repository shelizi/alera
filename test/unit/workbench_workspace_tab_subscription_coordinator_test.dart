import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_tab_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tab_subscription_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('retires removed watchers before forgetting layouts and adding new watchers', () async {
    final registry = WorkbenchTabSubscriptionRegistry();
    final oldStream = StreamController<List<WorkspaceTabRecord>>();
    final newStream = StreamController<List<WorkspaceTabRecord>>();
    addTearDown(oldStream.close);
    addTearDown(newStream.close);
    registry.watch(
      projectId: 'project',
      workspaceId: 'old',
      stream: oldStream.stream,
      onData: (_) {},
    );
    final events = <String>[];
    final coordinator = WorkbenchWorkspaceTabSubscriptionCoordinator(registry);

    coordinator.sync(
      projectId: 'project',
      workspaces: <Workspace>[_workspace('new')],
      cleanupRetiredWorkspace: (workspaceId) {
        events.add('cleanup:$workspaceId:${registry.contains(workspaceId)}');
      },
      forgetClearedLayouts: (workspaceIds) {
        events.add(
          'forget:${workspaceIds.join(',')}:${registry.contains('old')}',
        );
      },
      loadLayoutInBackground: (workspaceId) {
        events.add('load:$workspaceId:${registry.contains(workspaceId)}');
      },
      watchTabs: (workspaceId) {
        events.add('watch:$workspaceId');
        return newStream.stream;
      },
      onTabsChanged: (_, _) {},
    );

    expect(events, <String>[
      'cleanup:old:true',
      'forget:old:false',
      'load:new:false',
      'watch:new',
    ]);
    expect(registry.contains('old'), isFalse);
    expect(registry.contains('new'), isTrue);
  });

  test(
    'already-watched live workspace is neither loaded nor watched again',
    () {
      final registry = WorkbenchTabSubscriptionRegistry();
      final stream = StreamController<List<WorkspaceTabRecord>>();
      addTearDown(stream.close);
      registry.watch(
        projectId: 'project',
        workspaceId: 'live',
        stream: stream.stream,
        onData: (_) {},
      );
      final events = <String>[];

      WorkbenchWorkspaceTabSubscriptionCoordinator(registry).sync(
        projectId: 'project',
        workspaces: <Workspace>[_workspace('live')],
        cleanupRetiredWorkspace: (workspaceId) =>
            events.add('cleanup:$workspaceId'),
        forgetClearedLayouts: (workspaceIds) =>
            events.add('forget:${workspaceIds.length}'),
        loadLayoutInBackground: (workspaceId) =>
            events.add('load:$workspaceId'),
        watchTabs: (_) {
          events.add('watch');
          return stream.stream;
        },
        onTabsChanged: (_, _) {},
      );

      expect(events, <String>['forget:0']);
      expect(registry.contains('live'), isTrue);
    },
  );

  test('new watcher forwards tab snapshots with its workspace id', () async {
    final registry = WorkbenchTabSubscriptionRegistry();
    final stream = StreamController<List<WorkspaceTabRecord>>();
    addTearDown(stream.close);
    final received = <String>[];

    WorkbenchWorkspaceTabSubscriptionCoordinator(registry).sync(
      projectId: 'project',
      workspaces: <Workspace>[_workspace('new')],
      cleanupRetiredWorkspace: (_) {},
      forgetClearedLayouts: (_) {},
      loadLayoutInBackground: (_) {},
      watchTabs: (_) => stream.stream,
      onTabsChanged: (workspaceId, tabs) =>
          received.add('$workspaceId:${tabs.single.id}'),
    );
    stream.add(<WorkspaceTabRecord>[_tab('tab', 'new')]);
    await Future<void>.delayed(Duration.zero);

    expect(received, <String>['new:tab']);
  });

  test('retirement cleanup failure stops before watcher cancellation and additions', () {
    final registry = WorkbenchTabSubscriptionRegistry();
    final oldStream = StreamController<List<WorkspaceTabRecord>>();
    addTearDown(oldStream.close);
    registry.watch(
      projectId: 'project',
      workspaceId: 'old',
      stream: oldStream.stream,
      onData: (_) {},
    );
    final events = <String>[];

    expect(
      () => WorkbenchWorkspaceTabSubscriptionCoordinator(registry).sync(
        projectId: 'project',
        workspaces: <Workspace>[_workspace('new')],
        cleanupRetiredWorkspace: (_) {
          events.add('cleanup');
          throw StateError('cleanup failed');
        },
        forgetClearedLayouts: (_) => events.add('forget'),
        loadLayoutInBackground: (_) => events.add('load'),
        watchTabs: (_) {
          events.add('watch');
          return const Stream<List<WorkspaceTabRecord>>.empty();
        },
        onTabsChanged: (_, _) {},
      ),
      throwsStateError,
    );

    expect(events, <String>['cleanup']);
    expect(registry.contains('old'), isTrue);
    expect(registry.contains('new'), isFalse);
  });
}

Workspace _workspace(String id) {
  final now = DateTime.utc(2026, 9, 11);
  return Workspace(
    id: id,
    projectId: 'project',
    name: id,
    path: 'repo/$id',
    createdAt: now,
    updatedAt: now,
    kind: WorkspaceKind.linked,
    status: WorkspaceStatus.active,
  );
}

WorkspaceTabRecord _tab(String id, String workspaceId) {
  final now = DateTime.utc(2026, 9, 11);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: workspaceId,
    title: id,
    createdAt: now,
    updatedAt: now,
  );
}
