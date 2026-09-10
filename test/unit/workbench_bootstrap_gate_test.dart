import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_bootstrap_gate.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('concurrent callers share the same in-flight bootstrap', () async {
    final gate = WorkbenchBootstrapGate();
    final actionGate = Completer<void>();
    var calls = 0;
    var secondCompleted = false;

    final first = gate.run(() async {
      calls += 1;
      await actionGate.future;
    });
    final second = gate
        .run(() async {
          calls += 1;
        })
        .whenComplete(() {
          secondCompleted = true;
        });

    await Future<void>.delayed(Duration.zero);
    expect(calls, 1);
    expect(secondCompleted, isFalse);

    actionGate.complete();
    await Future.wait<void>(<Future<void>>[first, second]);
    expect(calls, 1);
  });

  test('completed bootstrap is never executed again', () async {
    final gate = WorkbenchBootstrapGate();
    var calls = 0;

    await gate.run(() async {
      calls += 1;
    });
    await gate.run(() async {
      calls += 1;
    });

    expect(calls, 1);
  });

  test('failed bootstrap remains single-shot', () async {
    final gate = WorkbenchBootstrapGate();
    var calls = 0;

    Future<void> action() async {
      calls += 1;
      throw StateError('bootstrap failed');
    }

    await expectLater(gate.run(action), throwsStateError);
    await expectLater(gate.run(action), throwsStateError);
    expect(calls, 1);
  });
}
