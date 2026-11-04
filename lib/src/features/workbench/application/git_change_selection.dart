/// Selection state shared by the flat and tree Git change projections.
///
/// IDs include the staged area, so a path can appear twice without the two
/// rows sharing selection state.
final class GitChangeSelection {
  final Set<String> _ids = <String>{};
  String? _anchor;

  Set<String> get ids => Set<String>.unmodifiable(_ids);
  bool get isEmpty => _ids.isEmpty;
  bool contains(String id) => _ids.contains(id);

  void clear() {
    _ids.clear();
    _anchor = null;
  }

  void tap(
    String id, {
    required List<String> visibleIds,
    required bool shift,
    required bool toggle,
  }) {
    if (shift && _anchor != null) {
      final from = visibleIds.indexOf(_anchor!);
      final to = visibleIds.indexOf(id);
      if (from >= 0 && to >= 0) {
        final start = from < to ? from : to;
        final end = from > to ? from : to;
        _ids
          ..clear()
          ..addAll(visibleIds.getRange(start, end + 1));
        return;
      }
    }
    if (toggle) {
      if (!_ids.add(id)) {
        _ids.remove(id);
      }
    } else {
      _ids
        ..clear()
        ..add(id);
    }
    _anchor = id;
  }

  /// Right-click preserves a selected group; other rows become the sole
  /// target, matching the file explorer's context-menu behavior.
  void contextSelect(String id) {
    if (_ids.contains(id)) {
      return;
    }
    _ids
      ..clear()
      ..add(id);
    _anchor = id;
  }

  void retainVisible(Iterable<String> visibleIds) {
    final visible = visibleIds.toSet();
    _ids.removeWhere((id) => !visible.contains(id));
    if (_anchor != null && !visible.contains(_anchor)) {
      _anchor = null;
    }
  }
}
