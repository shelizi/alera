import 'dart:async';

import 'package:alera/src/shared/infra/performance/primary_startup.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('all independent cold-start tasks begin before any finishes', () async {
    final rust = Completer<void>();
    final codeForge = Completer<void>();
    final window = Completer<String>();
    final started = <String>[];
    final startup = initializePrimaryStartupDependencies<String>(
      initializeAleraRust: () {
        started.add('alera');
        return rust.future;
      },
      initializeCodeForge: () {
        started.add('codeforge');
        return codeForge.future;
      },
      restoreWindow: () {
        started.add('window');
        return window.future;
      },
      closeWindowOnFailure: (_) async => fail('should retain opened window'),
    );
    expect(started, ['alera', 'codeforge', 'window']);
    rust.complete();
    window.complete('restored main window');
    var completed = false;
    unawaited(startup.then((_) => completed = true));
    await Future<void>.delayed(Duration.zero);
    expect(completed, isFalse);
    codeForge.complete();
    expect(await startup, 'restored main window');
    expect(completed, isTrue);
  });

  test(
    'native failure waits for late window and closes its database',
    () async {
      final window = Completer<String>();
      final closed = <String>[];
      final startup = initializePrimaryStartupDependencies<String>(
        initializeAleraRust: () async => throw StateError('Rust load failure'),
        initializeCodeForge: () async {},
        restoreWindow: () => window.future,
        closeWindowOnFailure: (value) async => closed.add(value),
      );
      final failed = expectLater(startup, throwsStateError);
      window.complete('open DB');
      await failed;
      expect(closed, ['open DB']);
    },
  );

  test(
    'synchronous initialization errors do not skip other startup tasks',
    () async {
      final started = <String>[];
      final startup = initializePrimaryStartupDependencies<int>(
        initializeAleraRust: () {
          started.add('alera');
          throw StateError('load failed');
        },
        initializeCodeForge: () async => started.add('codeforge'),
        restoreWindow: () async {
          started.add('window');
          return 42;
        },
        closeWindowOnFailure: (value) async => expect(value, 42),
      );
      await expectLater(startup, throwsStateError);
      expect(started, ['alera', 'codeforge', 'window']);
    },
  );
}
