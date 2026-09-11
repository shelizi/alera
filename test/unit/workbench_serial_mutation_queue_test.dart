import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_serial_mutation_queue.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('runs actions one at a time in call order', () async {
    final queue = WorkbenchSerialMutationQueue();
    final events = <String>[];
    final started = Completer<void>();
    final release = Completer<void>();

    final first = queue.run<void>(() async {
      events.add('first-start');
      started.complete();
      await release.future;
      events.add('first-end');
    });
    await started.future;
    final second = queue.run<void>(() async => events.add('second'));
    await Future<void>.delayed(Duration.zero);

    expect(events, <String>['first-start']);

    release.complete();
    await Future.wait<void>(<Future<void>>[first, second]);
    expect(events, <String>['first-start', 'first-end', 'second']);
  });

  test('a failed action releases the next action', () async {
    final queue = WorkbenchSerialMutationQueue();
    final events = <String>[];

    await expectLater(
      queue.run<void>(() async {
        events.add('failed');
        throw StateError('failed');
      }),
      throwsStateError,
    );
    await queue.run<void>(() async => events.add('next'));

    expect(events, <String>['failed', 'next']);
  });
}
