import 'dart:async';

import 'package:alera/src/features/workbench/application/detached_tab_bootstrap.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'native, logger, database and window startup overlap for editor',
    () async {
      final logger = Completer<void>();
      final window = Completer<void>();
      final aleraRust = Completer<void>();
      final codeForge = Completer<void>();
      final database = Completer<String>();
      final started = <String>[];

      final loading = initializeDetachedTabDependencies<String>(
        kind: WorkspaceTabKind.editor,
        configureLogging: () {
          started.add('logger');
          return logger.future;
        },
        initializeWindow: () {
          started.add('window');
          return window.future;
        },
        initializeAleraRust: () {
          started.add('alera-rust');
          return aleraRust.future;
        },
        initializeCodeForge: () {
          started.add('code-forge');
          return codeForge.future;
        },
        openDatabase: () {
          started.add('database');
          return database.future;
        },
        closeDatabase: (_) async => fail('successful bootstrap must retain db'),
      );

      // No task is gated on completion of a prior task.
      expect(started, [
        'logger',
        'window',
        'alera-rust',
        'code-forge',
        'database',
      ]);
      logger.complete();
      window.complete();
      aleraRust.complete();
      database.complete('child database');
      var finished = false;
      unawaited(loading.then((_) => finished = true));
      await Future<void>.delayed(Duration.zero);
      expect(finished, isFalse);
      codeForge.complete();
      expect(await loading, 'child database');
      expect(finished, isTrue);
    },
  );

  test('terminal does not start CodeForge while other work overlaps', () async {
    final logger = Completer<void>();
    final aleraRust = Completer<void>();
    final started = <String>[];
    final result = initializeDetachedTabDependencies<int>(
      kind: WorkspaceTabKind.terminal,
      configureLogging: () {
        started.add('logger');
        return logger.future;
      },
      initializeWindow: () async => started.add('window'),
      initializeAleraRust: () {
        started.add('alera-rust');
        return aleraRust.future;
      },
      initializeCodeForge: () async => fail('terminal has no CodeForge editor'),
      openDatabase: () async {
        started.add('database');
        return 1;
      },
      closeDatabase: (_) async => fail('do not close successful DB'),
    );
    expect(started, ['logger', 'window', 'alera-rust', 'database']);
    aleraRust.complete();
    logger.complete();
    expect(await result, 1);
  });

  test(
    'a failed native engine waits for late database then closes it',
    () async {
      final pendingDatabase = Completer<String>();
      var closedDatabase = '';
      final result = initializeDetachedTabDependencies<String>(
        kind: WorkspaceTabKind.markdownViewer,
        configureLogging: () async {},
        initializeWindow: () async {},
        initializeAleraRust: () async => throw StateError('native unavailable'),
        initializeCodeForge: () async => fail('unexpected CodeForge init'),
        openDatabase: () => pendingDatabase.future,
        closeDatabase: (database) async {
          closedDatabase = database;
        },
      );
      final assertion = expectLater(result, throwsStateError);
      // The database resolves after the failing engine. Cleanup must not leak
      // this newly opened SQLite handle while the failed window stays visible.
      pendingDatabase.complete('opened after native failure');
      await assertion;
      expect(closedDatabase, 'opened after native failure');
    },
  );
}
