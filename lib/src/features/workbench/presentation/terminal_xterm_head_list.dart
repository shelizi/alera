import 'dart:collection';

/// Indexed FIFO storage: release removed references immediately, shift the
/// surviving rows only when the unused prefix is large enough to amortize it.
final class TerminalXtermHeadList<T> extends ListBase<T> {
  static const int _minCompactionHead = 1024;

  final List<T?> _storage = <T?>[];
  int _head = 0;

  int get storageHead => _head;

  @override
  int get length => _storage.length - _head;

  @override
  set length(int value) {
    if (value < 0) {
      throw RangeError.range(value, 0, null, 'value');
    }
    final current = length;
    if (value > current) {
      throw UnsupportedError('Grow the terminal head list with add/addAll.');
    }
    if (value == 0) {
      clear();
      return;
    }
    if (value == current) return;
    _storage.length = _head + value;
  }

  @override
  T operator [](int index) {
    RangeError.checkValidIndex(index, this);
    return _storage[_head + index] as T;
  }

  @override
  void operator []=(int index, T value) {
    RangeError.checkValidIndex(index, this);
    _storage[_head + index] = value;
  }

  @override
  void add(T value) => _storage.add(value);

  @override
  void addAll(Iterable<T> iterable) => _storage.addAll(iterable);

  @override
  void clear() {
    _storage.clear();
    _head = 0;
  }

  void trimStart(int count) {
    RangeError.checkValueInInterval(count, 0, length, 'count');
    if (count == 0) return;
    final end = _head + count;
    for (var index = _head; index < end; index++) {
      _storage[index] = null;
    }
    _head = end;
    if (length == 0) {
      clear();
      return;
    }
    _compactIfNeeded();
  }

  void _compactIfNeeded() {
    if (_head < _minCompactionHead || _head * 2 < _storage.length) return;
    final retained = _storage.sublist(_head);
    _storage
      ..clear()
      ..addAll(retained);
    _head = 0;
  }
}
