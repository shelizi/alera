// lib/src/controller/expansion_controller.dart
import 'package:directory_tree/directory_tree.dart';
import 'package:flutter/foundation.dart';

/// Manages the state of expanded folders.
///
/// Wraps an [ExpansionSet] and notifies listeners when the set changes.
/// Use [performBatch] to apply multiple changes (e.g., "Collapse All") while only
/// rebuilding the UI once.
class ExpansionController extends ChangeNotifier {
  /// Creates a controller with an optional initial state.
  ///
  /// * [initiallyExpanded]: A set of IDs that should be expanded by default.
  /// * [state]: A full [ExpansionSet] object (advanced usage).
  ExpansionController({Set<String>? initiallyExpanded, ExpansionSet? state})
      : _state = state ?? ExpansionSet(initiallyExpanded: initiallyExpanded);

  final ExpansionSet _state;
  bool _batching = false;
  bool _dirty = false;

  /// Executes [updates] synchronously but defers listener notification until completion.
  ///
  /// Use this when modifying multiple nodes at once to prevent unnecessary re-renders.
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

  /// Returns true if the folder with [id] is currently open.
  bool isExpanded(String id) => _state.isExpanded(id);

  /// Manually sets the expansion state of a specific node.
  ///
  /// Notifies listeners if the state actually changes.
  void setExpanded(String id, bool expanded) {
    _markChanged(_state.setExpanded(id, expanded));
  }

  /// Inverts the expansion state of the node (open -> closed, or closed -> open).
  void toggle(String id) {
    _markChanged(_state.toggle(id));
  }

  /// Expands all nodes in the provided [ids] list.
  ///
  /// Operations are batched to emit a single change notification.
  void expandAll(Iterable<String> ids) {
    performBatch(() {
      _markChanged(_state.expandAll(ids));
    });
  }

  /// Closes all currently expanded folders.
  void collapseAll() {
    performBatch(() {
      _markChanged(_state.collapseAll());
    });
  }

  /// Returns a live view of the set of currently expanded node IDs.
  Set<String> get expandedIds => _state.expandedIds;

  /// Keep only ids that still exist in the new tree.
  void retainWhere(bool Function(String id) test) {
    _markChanged(_state.retainWhere(test));
  }

  /// Returns a snapshot of the current expansion state.
  ///
  /// Useful for saving/restoring state or debugging.
  ExpansionSet get state => ExpansionSet(initiallyExpanded: _state.expandedIds);

  void _markChanged(bool changed) {
    if (!changed) return;
    if (_batching) {
      _dirty = true;
    } else {
      notifyListeners();
    }
  }
}
