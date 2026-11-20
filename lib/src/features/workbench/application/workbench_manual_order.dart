/// Pure sidebar preference mutation: preserve saved order for invisible rows,
/// seed newly discovered IDs from the visible list, and keep stable IDs.
/// A drop below the source places it after the target; a drop above places
/// it before the target. No Project, Workspace or Git metadata is mutated.
List<String>? moveSidebarOrderId({
  required List<String> savedOrder,
  required Iterable<String> visibleOrder,
  required Iterable<String> knownIds,
  required String sourceId,
  required String targetId,
  required bool isManual,
}) {
  if (sourceId == targetId) return null;
  final known = knownIds.toSet();
  if (!known.contains(sourceId) || !known.contains(targetId)) return null;
  final result = <String>[];
  final seen = <String>{};
  void append(Iterable<String> ids) {
    for (final id in ids) {
      if (known.contains(id) && seen.add(id)) result.add(id);
    }
  }

  if (isManual) append(savedOrder);
  append(visibleOrder);
  if (!isManual) append(savedOrder);
  append(knownIds);
  final from = result.indexOf(sourceId);
  final to = result.indexOf(targetId);
  if (from < 0 || to < 0) return null;
  result.removeAt(from);
  final targetIndex = result.indexOf(targetId);
  result.insert(from < to ? targetIndex + 1 : targetIndex, sourceId);
  return result;
}
