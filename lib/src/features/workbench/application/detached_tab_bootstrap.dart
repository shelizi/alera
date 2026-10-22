import 'dart:async';

import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

/// Independent startup tasks for a detached tab. Keep a child process's
/// expensive initialization off the critical path of other independent work.
/// All tasks must finish before the parent receives READY.
Future<T> initializeDetachedTabDependencies<T>({
  required WorkspaceTabKind kind,
  required Future<void> Function() configureLogging,
  required Future<void> Function() initializeWindow,
  required Future<void> Function() initializeAleraRust,
  required Future<void> Function() initializeCodeForge,
  required Future<T> Function() openDatabase,
  required Future<void> Function(T database) closeDatabase,
}) async {
  T? database;
  try {
    // Future.wait defaults to eagerError=false: if one subsystem fails, wait
    // for the others to settle so a late successful database open can still
    // be closed before reporting a failed child launch to the parent.
    await Future.wait<void>(<Future<void>>[
      Future<void>.sync(configureLogging),
      Future<void>.sync(initializeWindow),
      Future<void>.sync(initializeAleraRust),
      if (detachedTabRequiresCodeForge(kind))
        Future<void>.sync(initializeCodeForge),
      Future<void>.sync(() async {
        database = await openDatabase();
      }),
    ]);
    return database as T;
  } catch (_) {
    if (database case final opened?) {
      await closeDatabase(opened);
    }
    rethrow;
  }
}

/// A separate Rust editor engine is required only for CodeForge surfaces.
bool detachedTabRequiresCodeForge(WorkspaceTabKind kind) =>
    kind == WorkspaceTabKind.editor || kind == WorkspaceTabKind.gitDiff;
