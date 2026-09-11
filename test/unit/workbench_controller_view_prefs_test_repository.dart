part of 'workbench_controller_test.dart';

class _FakeWorkbenchViewPrefsRepository
    implements WorkbenchViewPrefsRepository {
  WorkbenchViewPrefs prefs = .defaults;
  Object? loadError;
  Object? saveError;
  int saveCount = 0;
  Future<WorkbenchViewPrefs>? loadOverride;
  Completer<void>? saveStarted;
  Completer<void>? saveRelease;
  Completer<void>? saveCompleted;

  @override
  Stream<WorkbenchViewPrefs> get changes => const Stream.empty();

  @override
  Future<WorkbenchViewPrefs> load() async {
    if (loadError case final Object error) {
      throw error;
    }
    final override = loadOverride;
    if (override != null) {
      return override;
    }
    return prefs;
  }

  @override
  Future<void> save(WorkbenchViewPrefs prefs) async {
    saveCount += 1;
    if (saveError case final Object error) {
      throw error;
    }
    final started = saveStarted;
    final release = saveRelease;
    final completed = saveCompleted;
    if (started != null) {
      saveStarted = null;
      saveRelease = null;
      saveCompleted = null;
      if (!started.isCompleted) {
        started.complete();
      }
      if (release != null) {
        await release.future;
      }
    }
    this.prefs = prefs;
    if (completed != null && !completed.isCompleted) {
      completed.complete();
    }
  }
}
