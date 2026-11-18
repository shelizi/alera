import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';

/// A common preference cannot be written to the authoritative Rust Host.
/// Callers must restore the last persisted view instead of silently
/// creating a second, UI-local copy of the same value.
final class SharedWorkbenchPreferenceUnavailable implements Exception {
  const SharedWorkbenchPreferenceUnavailable(this.preference);
  final String preference;

  @override
  String toString() => 'Shared $preference requires the Rust Runtime Host';
}

/// Runtime Host events only describe shared preferences. Replaying their
/// local Drift snapshot must not overwrite an in-progress Classic-only UI
/// selection (such as the active Source Control/Explorer panel).
///
/// Repositories without this capability still publish full snapshots.
abstract interface class SharedWorkbenchViewPrefsEventMerger {
  WorkbenchViewPrefs mergeIncomingSharedPrefs({
    required WorkbenchViewPrefs current,
    required WorkbenchViewPrefs incoming,
  });
}

abstract interface class WorkbenchViewPrefsRepository {
  Stream<WorkbenchViewPrefs> get changes;

  Future<WorkbenchViewPrefs> load();

  Future<void> save(WorkbenchViewPrefs prefs);
}
