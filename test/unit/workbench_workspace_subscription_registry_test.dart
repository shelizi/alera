import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_workspace_subscription_registry.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('tracks one workspace watcher per project', () {
    final registry = WorkbenchWorkspaceSubscriptionRegistry();
    final controller = StreamController<List<Workspace>>();
    addTearDown(controller.close);

    expect(
      registry.watch(
        projectId: 'project',
        stream: controller.stream,
        onData: (_) {},
      ),
      isTrue,
    );
    expect(
      registry.watch(
        projectId: 'project',
        stream: const Stream<List<Workspace>>.empty(),
        onData: (_) {},
      ),
      isFalse,
    );
    expect(registry.contains('project'), isTrue);
    expect(registry.projectIds, <String>['project']);
  });

  test(
    'completed watchers are forgotten so the project can re-subscribe',
    () async {
      final registry = WorkbenchWorkspaceSubscriptionRegistry();
      final first = StreamController<List<Workspace>>();

      registry.watch(
        projectId: 'project',
        stream: first.stream,
        onData: (_) {},
      );
      expect(registry.contains('project'), isTrue);

      await first.close();
      await _flush();
      expect(registry.contains('project'), isFalse);

      final second = StreamController<List<Workspace>>();
      addTearDown(second.close);
      expect(
        registry.watch(
          projectId: 'project',
          stream: second.stream,
          onData: (_) {},
        ),
        isTrue,
      );
      expect(registry.contains('project'), isTrue);
    },
  );

  test('cancelProject forgets the project and cancels its watcher', () async {
    var cancelCalls = 0;
    final controller = StreamController<List<Workspace>>(
      onCancel: () {
        cancelCalls += 1;
      },
    );
    addTearDown(controller.close);
    final registry = WorkbenchWorkspaceSubscriptionRegistry();

    registry.watch(
      projectId: 'project',
      stream: controller.stream,
      onData: (_) {},
    );
    registry.cancelProject('project');
    await _flush();

    expect(registry.contains('project'), isFalse);
    expect(registry.projectIds, isEmpty);
    expect(cancelCalls, 1);
  });

  test('old watcher completion cannot forget a replacement watcher', () async {
    final cancellationGate = Completer<void>();
    final first = StreamController<List<Workspace>>(
      onCancel: () => cancellationGate.future,
    );
    final second = StreamController<List<Workspace>>();
    addTearDown(first.close);
    addTearDown(second.close);
    final registry = WorkbenchWorkspaceSubscriptionRegistry();

    registry.watch(projectId: 'project', stream: first.stream, onData: (_) {});
    registry.cancelProject('project');
    expect(
      registry.watch(
        projectId: 'project',
        stream: second.stream,
        onData: (_) {},
      ),
      isTrue,
    );

    cancellationGate.complete();
    await _flush();

    expect(registry.contains('project'), isTrue);
    expect(registry.projectIds, <String>['project']);
  });

  test('cancelAll clears every tracked project', () async {
    final first = StreamController<List<Workspace>>();
    final second = StreamController<List<Workspace>>();
    addTearDown(first.close);
    addTearDown(second.close);
    final registry = WorkbenchWorkspaceSubscriptionRegistry();

    registry.watch(projectId: 'first', stream: first.stream, onData: (_) {});
    registry.watch(projectId: 'second', stream: second.stream, onData: (_) {});

    registry.cancelAll();
    await _flush();

    expect(registry.contains('first'), isFalse);
    expect(registry.contains('second'), isFalse);
    expect(registry.projectIds, isEmpty);
  });
}

Future<void> _flush() => Future<void>.delayed(Duration.zero);
