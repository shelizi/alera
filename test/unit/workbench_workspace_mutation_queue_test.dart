import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_workspace_mutation_queue.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('serializes actions for the same workspace', () async {
    final queue = WorkbenchWorkspaceMutationQueue();
    final events = <String>[];
    final started = Completer<void>();
    final release = Completer<void>();

    final first = queue.run<void>(
      workspaceId: 'workspace',
      action: () async {
        events.add('first-start');
        started.complete();
        await release.future;
        events.add('first-end');
      },
    );
    await started.future;
    final second = queue.run<void>(
      workspaceId: 'workspace',
      action: () async => events.add('second'),
    );
    await Future<void>.delayed(Duration.zero);

    expect(events, <String>['first-start']);

    release.complete();
    await Future.wait<void>(<Future<void>>[first, second]);
    expect(events, <String>['first-start', 'first-end', 'second']);
  });

  test('different workspaces remain concurrent', () async {
    final queue = WorkbenchWorkspaceMutationQueue();
    final events = <String>[];
    final started = Completer<void>();
    final release = Completer<void>();

    final blocked = queue.run<void>(
      workspaceId: 'workspace-a',
      action: () async {
        events.add('a-start');
        started.complete();
        await release.future;
        events.add('a-end');
      },
    );
    await started.future;
    await queue.run<void>(
      workspaceId: 'workspace-b',
      action: () async => events.add('b'),
    );

    expect(events, <String>['a-start', 'b']);

    release.complete();
    await blocked;
  });

  test('a failed action releases the following action', () async {
    final queue = WorkbenchWorkspaceMutationQueue();
    final events = <String>[];

    await expectLater(
      queue.run<void>(
        workspaceId: 'workspace',
        action: () async {
          events.add('failed');
          throw StateError('failed');
        },
      ),
      throwsStateError,
    );
    await queue.run<void>(
      workspaceId: 'workspace',
      action: () async => events.add('next'),
    );

    expect(events, <String>['failed', 'next']);
  });
}
