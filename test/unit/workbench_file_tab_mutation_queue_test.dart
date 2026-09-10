import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_file_tab_mutation_queue.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('runs mutations in submission order', () async {
    final queue = WorkbenchFileTabMutationQueue();
    final firstGate = Completer<void>();
    final events = <String>[];

    final first = queue.run(() async {
      events.add('first-start');
      await firstGate.future;
      events.add('first-end');
      return 1;
    });
    final second = queue.run(() async {
      events.add('second');
      return 2;
    });
    final third = queue.run(() async {
      events.add('third');
      return 3;
    });

    await Future<void>.delayed(Duration.zero);
    expect(events, <String>['first-start']);

    firstGate.complete();
    expect(await first, 1);
    expect(await second, 2);
    expect(await third, 3);
    expect(events, <String>['first-start', 'first-end', 'second', 'third']);
  });

  test('a failed mutation does not block the next mutation', () async {
    final queue = WorkbenchFileTabMutationQueue();
    final gate = Completer<void>();
    final events = <String>[];

    final first = queue.run<void>(() async {
      events.add('first-start');
      await gate.future;
      throw StateError('file open failed');
    });
    final second = queue.run(() async {
      events.add('second');
      return 'ok';
    });

    gate.complete();
    await expectLater(first, throwsStateError);
    expect(await second, 'ok');
    expect(events, <String>['first-start', 'second']);
  });

  test('accepts new work after the queue drains', () async {
    final queue = WorkbenchFileTabMutationQueue();
    var calls = 0;

    expect(await queue.run(() async => ++calls), 1);
    expect(await queue.run(() async => ++calls), 2);
  });
}
