import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_workspace_tab_closing_scope.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'marks a workspace as closing only while the action is active',
    () async {
      final scope = WorkbenchWorkspaceTabClosingScope();
      final gate = Completer<void>();

      final future = scope.run('workspace', () async {
        expect(scope.isClosing('workspace'), isTrue);
        await gate.future;
        return 42;
      });

      expect(scope.isClosing('workspace'), isTrue);
      gate.complete();

      expect(await future, 42);
      expect(scope.isClosing('workspace'), isFalse);
    },
  );

  test(
    'overlapping scopes keep the workspace closing until both finish',
    () async {
      final scope = WorkbenchWorkspaceTabClosingScope();
      final firstGate = Completer<void>();
      final secondGate = Completer<void>();

      final first = scope.run('workspace', () => firstGate.future);
      final second = scope.run('workspace', () => secondGate.future);

      expect(scope.isClosing('workspace'), isTrue);

      firstGate.complete();
      await first;
      expect(scope.isClosing('workspace'), isTrue);

      secondGate.complete();
      await second;
      expect(scope.isClosing('workspace'), isFalse);
    },
  );

  test('different workspaces are tracked independently', () async {
    final scope = WorkbenchWorkspaceTabClosingScope();
    final firstGate = Completer<void>();
    final secondGate = Completer<void>();

    final first = scope.run('first', () => firstGate.future);
    final second = scope.run('second', () => secondGate.future);

    expect(scope.isClosing('first'), isTrue);
    expect(scope.isClosing('second'), isTrue);

    firstGate.complete();
    await first;
    expect(scope.isClosing('first'), isFalse);
    expect(scope.isClosing('second'), isTrue);

    secondGate.complete();
    await second;
  });

  test('failures release the closing scope', () async {
    final scope = WorkbenchWorkspaceTabClosingScope();

    await expectLater(
      scope.run<void>('workspace', () async {
        throw StateError('close failed');
      }),
      throwsStateError,
    );

    expect(scope.isClosing('workspace'), isFalse);
  });
}
