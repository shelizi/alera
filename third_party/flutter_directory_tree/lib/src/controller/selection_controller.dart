// lib/src/controller/selection_controller.dart
import 'package:directory_tree/directory_tree.dart'
    show SelectionMode, SelectionSet;
import 'package:flutter/foundation.dart';

export 'package:directory_tree/directory_tree.dart' show SelectionMode;

/// Manages single or multiple selection states.
///
/// Maintains a [SelectionSet] and notifies the [DirectoryTreeController] on changes.
/// Supports different modes via [SelectionMode]:
/// * [SelectionMode.single]: Only one node selected at a time.
/// * [SelectionMode.multiple]: Allows arbitrary collections of nodes.
class SelectionController extends ChangeNotifier {
  /// Creates a selection controller.
  ///
  /// * [mode]: Defines whether single or multiple items can be selected.
  /// * [state]: Optional initial state snapshot.
  SelectionController(
      {SelectionMode mode = SelectionMode.single, SelectionSet? state})
      : _state = state ?? SelectionSet(mode: mode) {
    // Ensure injected state follows the requested mode.
    _state.mode = mode;
  }

  final SelectionSet _state;
  bool _batching = false;
  bool _dirty = false;

  /// The active selection strategy.
  ///
  /// * [SelectionMode.single]: Enforces at most one selected item.
  /// * [SelectionMode.multiple]: Allows multiple items to be selected.
  SelectionMode get mode => _state.mode;

  set mode(SelectionMode value) => _state.mode = value;

  /// Returns true if [id] is in the currently selected set.
  bool isSelected(String id) => _state.isSelected(id);

  /// The set of currently selected node IDs.
  Set<String> get selectedIds => _state.selectedIds;

  /// Deselects everything else and selects [id].
  ///
  /// If [id] was already selected and it was the *only* selection, no change occurs.
  void selectOnly(String id) {
    if (_state.selectOnly(id)) {
      _emitChange();
    }
  }

  /// Toggles the selection state of [id].
  ///
  /// * In [SelectionMode.single], this behaves like [selectOnly] unless the item is already selected.
  /// * In [SelectionMode.multiple], it adds or removes [id] from the set.
  void toggle(String id) {
    if (_state.toggle(id)) {
      _emitChange();
    }
  }

  /// Deselects all nodes.
  void clear() {
    if (_state.clear()) {
      _emitChange();
    }
  }

  /// Selects a contiguous range of nodes between [anchorId] and [toId].
  ///
  /// This replicates standard "Shift+Click" behavior:
  /// 1. Finds the indices of [anchorId] and [toId] within [orderedVisibleIds].
  /// 2. Selects every node between those indices inclusive.
  void selectRange(
      List<String> orderedVisibleIds, String anchorId, String toId) {
    if (_state.selectRange(orderedVisibleIds, anchorId, toId)) {
      _emitChange();
    }
  }

  /// Keep only ids that still exist in the new tree.
  void retainWhere(bool Function(String id) test) {
    if (_state.retainWhere(test)) {
      _emitChange();
    }
  }

  /// Perform several selection updates while emitting at most one notification.
  void performBatch(void Function() updates) {
    if (_batching) {
      updates();
      return;
    }
    _batching = true;
    try {
      updates();
    } finally {
      _batching = false;
      if (_dirty) {
        _dirty = false;
        notifyListeners();
      }
    }
  }

  /// Add [ids] to the current selection.
  void addAll(Iterable<String> ids) {
    if (_state.addAll(ids)) {
      _emitChange();
    }
  }

  /// Remove [ids] from the current selection.
  void removeAll(Iterable<String> ids) {
    if (_state.removeAll(ids)) {
      _emitChange();
    }
  }

  /// Returns a snapshot of the current selection state.
  SelectionSet get state {
    final snapshot = SelectionSet(mode: _state.mode);
    if (_state.selectedIds.isNotEmpty) {
      snapshot.addAll(_state.selectedIds);
    }
    return snapshot;
  }

  void _emitChange() {
    if (_batching) {
      _dirty = true;
    } else {
      notifyListeners();
    }
  }
}
