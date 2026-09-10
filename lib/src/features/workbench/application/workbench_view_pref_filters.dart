import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';

enum WorkbenchFilterMutation { toggle, add, remove, clear }

WorkbenchViewPrefs? nextWorkbenchProjectFilterPrefs({
  required WorkbenchViewPrefs prefs,
  String? id,
  required WorkbenchFilterMutation mutation,
}) {
  final selectedProjectIds = _nextFilterIds(
    prefs.selectedProjectIds,
    id: id,
    mutation: mutation,
  );
  return selectedProjectIds == null
      ? null
      : prefs.copyWith(selectedProjectIds: selectedProjectIds);
}

WorkbenchViewPrefs? nextWorkbenchTagFilterPrefs({
  required WorkbenchViewPrefs prefs,
  String? id,
  required WorkbenchFilterMutation mutation,
}) {
  final selectedTagIds = _nextFilterIds(
    prefs.selectedTagIds,
    id: id,
    mutation: mutation,
  );
  return selectedTagIds == null
      ? null
      : prefs.copyWith(selectedTagIds: selectedTagIds);
}

Set<String>? _nextFilterIds(
  Set<String> current, {
  required String? id,
  required WorkbenchFilterMutation mutation,
}) {
  switch (mutation) {
    case WorkbenchFilterMutation.toggle:
      final value = _requireFilterId(id);
      final next = Set<String>.from(current);
      if (!next.add(value)) {
        next.remove(value);
      }
      return next;
    case WorkbenchFilterMutation.add:
      final value = _requireFilterId(id);
      if (current.contains(value)) {
        return null;
      }
      return <String>{...current, value};
    case WorkbenchFilterMutation.remove:
      final value = _requireFilterId(id);
      if (!current.contains(value)) {
        return null;
      }
      return <String>{
        for (final candidate in current)
          if (candidate != value) candidate,
      };
    case WorkbenchFilterMutation.clear:
      return current.isEmpty ? null : const <String>{};
  }
}

String _requireFilterId(String? id) {
  if (id == null) {
    throw ArgumentError.notNull('id');
  }
  return id;
}
