import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_tab_subscription_registry.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('tracks workspace subscriptions by project', () {
    final registry = WorkbenchTabSubscriptionRegistry();
    final first = StreamController<List<WorkspaceTabRecord>>();
    final second = StreamController<List<WorkspaceTabRecord>>();
    addTearDown(first.close);
    addTearDown(second.close);

    registry.watch(
      projectId: 'project-a',
      workspaceId: 'workspace-a',
      stream: first.stream,
      onData: (_) {},
    );
    registry.watch(
      projectId: 'project-b',
      workspaceId: 'workspace-b',
      stream: second.stream,
      onData: (_) {},
    );

    expect(registry.contains('workspace-a'), isTrue);
    expect(registry.workspaceIdsForProject('project-a'), <String>[
      'workspace-a',
    ]);
    expect(registry.workspaceIdsForProject('project-b'), <String>[
      'workspace-b',
    ]);
  });

  test(
    'completed watchers are forgotten so the workspace can re-subscribe',
    () async {
      final registry = WorkbenchTabSubscriptionRegistry();
      final first = StreamController<List<WorkspaceTabRecord>>();

      registry.watch(
        projectId: 'project',
        workspaceId: 'workspace',
        stream: first.stream,
        onData: (_) {},
      );
      expect(registry.contains('workspace'), isTrue);

      await first.close();
      await _flush();
      expect(registry.contains('workspace'), isFalse);

      final second = StreamController<List<WorkspaceTabRecord>>();
      addTearDown(second.close);
      registry.watch(
        projectId: 'project',
        workspaceId: 'workspace',
        stream: second.stream,
        onData: (_) {},
      );

      expect(registry.contains('workspace'), isTrue);
    },
  );

  test('cancelWorkspace forgets ownership and cancels the listener', () async {
    var cancelCalls = 0;
    final controller = StreamController<List<WorkspaceTabRecord>>(
      onCancel: () {
        cancelCalls += 1;
      },
    );
    addTearDown(controller.close);
    final registry = WorkbenchTabSubscriptionRegistry();

    registry.watch(
      projectId: 'project',
      workspaceId: 'workspace',
      stream: controller.stream,
      onData: (_) {},
    );
    registry.cancelWorkspace('workspace');
    await _flush();

    expect(registry.contains('workspace'), isFalse);
    expect(registry.workspaceIdsForProject('project'), isEmpty);
    expect(cancelCalls, 1);
  });

  test('cancelAll clears every tracked workspace', () async {
    final first = StreamController<List<WorkspaceTabRecord>>();
    final second = StreamController<List<WorkspaceTabRecord>>();
    addTearDown(first.close);
    addTearDown(second.close);
    final registry = WorkbenchTabSubscriptionRegistry();

    registry.watch(
      projectId: 'project',
      workspaceId: 'first',
      stream: first.stream,
      onData: (_) {},
    );
    registry.watch(
      projectId: 'project',
      workspaceId: 'second',
      stream: second.stream,
      onData: (_) {},
    );

    registry.cancelAll();
    await _flush();

    expect(registry.contains('first'), isFalse);
    expect(registry.contains('second'), isFalse);
    expect(registry.workspaceIdsForProject('project'), isEmpty);
  });
}

Future<void> _flush() => Future<void>.delayed(Duration.zero);
