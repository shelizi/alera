import 'dart:async';

/// Main-window cold bootstrap: the two independent Rust FFI engines and
/// desktop window/SQLite restoration must finish before the real app appears,
/// but none needs to wait for the others to START.
///
/// Future.wait intentionally waits for every result on failure. A window DB
/// opened after a Rust initialization error is closed before the caller sees
/// that error, preventing a stranded SQLite handle after partial bootstrap.
Future<T> initializePrimaryStartupDependencies<T>({
  required Future<void> Function() initializeAleraRust,
  required Future<void> Function() initializeCodeForge,
  required Future<T> Function() restoreWindow,
  required Future<void> Function(T) closeWindowOnFailure,
}) async {
  T? restoredWindow;
  try {
    await Future.wait<void>(<Future<void>>[
      Future<void>.sync(initializeAleraRust),
      Future<void>.sync(initializeCodeForge),
      Future<void>.sync(() async {
        restoredWindow = await restoreWindow();
      }),
    ]);
    return restoredWindow as T;
  } catch (_) {
    if (restoredWindow case final opened?) {
      await closeWindowOnFailure(opened);
    }
    rethrow;
  }
}
