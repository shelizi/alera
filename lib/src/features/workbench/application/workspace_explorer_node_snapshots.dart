import 'dart:collection';

/// Owns the pending node graph and publishes immutable, ordered snapshots.
///
/// Small edits share a single immutable base, not a chain of older
/// snapshots. Only the bounded overlay is copied on publication. A full
/// replacement or a large accumulated edit starts a new compact base.
final class WorkspaceExplorerNodeSnapshots<V extends Object> {
  static const int _maxOverlayRecords = 512;
  static const int _minSharedBaseSize = 256;

  Map<String, V> _base = const {};
  final Map<String, V> _changes = {};
  // A removed base key must not reappear at its old insertion position,
  // even if a later update recreates the same ID.
  final Set<String> _removedBaseKeys = {};
  final Set<String> _appendedKeys = {};
  Map<String, V>? _published;

  // Independently track the delta since the last UI handoff, including
  // edits that were compacted into a new base. Bound the invalidation plan.
  Set<String>? _pendingChangedKeys;

  void _recordChange(String key) {
    final pending = _pendingChangedKeys;
    if (pending == null) return;
    pending.add(key);
    if (pending.length > 512) _pendingChangedKeys = null;
  }

  /// Drain a complete change set after snapshot() and before publication.
  /// Null indicates a full replacement or too many accumulated edits.
  Set<String>? takeChangedKeys() {
    final pending = _pendingChangedKeys;
    _pendingChangedKeys = <String>{};
    return pending;
  }

  void put(String key, V value) {
    if (!_base.containsKey(key) || _removedBaseKeys.contains(key)) {
      _appendedKeys.add(key);
    }
    _changes[key] = value;
    _recordChange(key);
    _published = null;
  }

  void remove(String key) {
    final hadChange = _changes.remove(key) != null;
    final hadBase = _base.containsKey(key) && _removedBaseKeys.add(key);
    if (!hadChange && !hadBase) return;
    _appendedKeys.remove(key);
    _recordChange(key);
    _published = null;
  }

  void clear() => replace(const []);

  void replace(Iterable<MapEntry<String, V>> entries) {
    // The backing map is created here and never mutated after publication.
    _base = UnmodifiableMapView(Map<String, V>.fromEntries(entries));
    _changes.clear();
    _removedBaseKeys.clear();
    _appendedKeys.clear();
    _published = _base;
    _pendingChangedKeys = null;
  }

  /// Returns the exact same immutable snapshot until another edit arrives.
  Map<String, V> snapshot() {
    final existing = _published;
    if (existing != null) return existing;
    if (_changes.isEmpty && _removedBaseKeys.isEmpty) {
      return _published = _base;
    }

    // Keep lookup overhead bounded: every overlay refers directly to a
    // compact base, and the total number of retained delta records is capped.
    // Small maps use the ordinary compact representation.
    final records =
        _changes.length + _removedBaseKeys.length + _appendedKeys.length;
    if (_base.length < _minSharedBaseSize || records > _maxOverlayRecords) {
      final merged = <String, V>{
        for (final entry in _base.entries)
          if (!_removedBaseKeys.contains(entry.key))
            entry.key: _changes[entry.key] ?? entry.value,
        for (final key in _appendedKeys) key: _changes[key]!,
      };
      _base = UnmodifiableMapView(merged);
      _changes.clear();
      _removedBaseKeys.clear();
      _appendedKeys.clear();
      return _published = _base;
    }

    return _published = _ExplorerNodeOverlay<V>(
      _base,
      Map<String, V>.of(_changes),
      Set<String>.of(_removedBaseKeys),
      List<String>.of(_appendedKeys),
    );
  }
}

/// Private backing collections are owned by this snapshot and never
/// mutated. Iteration preserves LinkedHashMap insertion order, including
/// remove/reinsert and duplicate virtual-path lookup semantics.
final class _ExplorerNodeOverlay<V extends Object> extends MapBase<String, V> {
  _ExplorerNodeOverlay(
    this._base,
    this._changes,
    this._removedBaseKeys,
    this._appendedKeys,
  );

  final Map<String, V> _base;
  final Map<String, V> _changes;
  final Set<String> _removedBaseKeys;
  final List<String> _appendedKeys;

  @override
  V? operator [](Object? key) {
    final changed = _changes[key];
    if (changed != null) return changed;
    if (_removedBaseKeys.isNotEmpty && _removedBaseKeys.contains(key)) {
      return null;
    }
    return _base[key];
  }

  @override
  bool containsKey(Object? key) => this[key] != null;

  @override
  int get length =>
      _base.length - _removedBaseKeys.length + _appendedKeys.length;

  @override
  Iterable<String> get keys sync* {
    for (final key in _base.keys) {
      if (!_removedBaseKeys.contains(key)) yield key;
    }
    yield* _appendedKeys;
  }

  @override
  Iterable<MapEntry<String, V>> get entries sync* {
    for (final entry in _base.entries) {
      if (!_removedBaseKeys.contains(entry.key)) {
        final changed = _changes[entry.key];
        yield changed == null ? entry : MapEntry(entry.key, changed);
      }
    }
    for (final key in _appendedKeys) {
      yield MapEntry(key, _changes[key]!);
    }
  }

  @override
  Iterable<V> get values sync* {
    for (final entry in entries) {
      yield entry.value;
    }
  }

  Never _immutable() => throw UnsupportedError('Immutable Explorer snapshot');

  @override
  void operator []=(String key, V value) => _immutable();

  @override
  V? remove(Object? key) => _immutable();

  @override
  void clear() => _immutable();

  @override
  void addAll(Map<String, V> other) => _immutable();

  @override
  void addEntries(Iterable<MapEntry<String, V>> newEntries) => _immutable();

  @override
  V putIfAbsent(String key, V Function() ifAbsent) => _immutable();

  @override
  V update(String key, V Function(V) update, {V Function()? ifAbsent}) =>
      _immutable();

  @override
  void updateAll(V Function(String, V) update) => _immutable();

  @override
  void removeWhere(bool Function(String, V) test) => _immutable();
}
