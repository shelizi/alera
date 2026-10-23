/// Prevents the main shared Engine from shutting down while its child Editor
/// still owns unsaved content. The multi-process fallback retains its own
/// independent process and is not handled here.
final class const SharedEngineDetachedQuitGuard() {
  /// Returns true only if no detached Editor is dirty or the user explicitly
  /// confirms save and every write has completed successfully.
  ///
  /// [tabIds] is snapshotted to tolerate a native close during the dialog.
  /// A second dirty check catches edits made while the save is pending.
  Future<bool> confirmAndSave({
    required Iterable<String> tabIds,
    required bool Function(String tabId) isDirty,
    required Future<bool> Function(List<String> dirtyTabIds) confirm,
    required Future<void> Function(String tabId) save,
    void Function(Object error)? onSaveFailure,
  }) async {
    final ids = tabIds.toSet();
    final dirty = ids.where(isDirty).toList(growable: false);
    if (dirty.isEmpty) return true;
    if (!await confirm(List<String>.unmodifiable(dirty))) return false;
    for (final id in dirty) {
      // A detached tab can be closed while the confirmation is visible.
      if (!isDirty(id)) continue;
      try {
        await save(id);
        if (isDirty(id)) {
          throw StateError('Editor $id still contains unsaved changes');
        }
      } catch (error) {
        onSaveFailure?.call(error);
        return false;
      }
    }
    if (ids.any(isDirty)) {
      onSaveFailure?.call(
        StateError('New unsaved changes appeared while quitting Alera'),
      );
      return false;
    }
    return true;
  }
}
