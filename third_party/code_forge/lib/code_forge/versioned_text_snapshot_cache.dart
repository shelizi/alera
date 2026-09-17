/// Reuses an in-flight or completed full-text snapshot while the document
/// version is unchanged.
///
/// The loader is intentionally supplied by the caller so this helper stays
/// independent from Rope/FFI and can be regression-tested without native code.
class VersionedTextSnapshotCache {
  int? _version;
  Future<String>? _snapshot;
  Object? _token;

  Future<String> get({
    required int version,
    required Future<String> Function() load,
  }) {
    final cached = _snapshot;
    if (_version == version && cached != null) {
      return cached;
    }

    final token = Object();
    final source = Future<String>.sync(load);
    _version = version;
    _token = token;
    final future = source.then(
      (value) => value,
      onError: (Object error, StackTrace stackTrace) {
        if (_version == version && identical(_token, token)) {
          invalidate();
        }
        Error.throwWithStackTrace(error, stackTrace);
      },
    );
    _snapshot = future;
    return future;
  }

  void invalidate() {
    _version = null;
    _snapshot = null;
    _token = null;
  }
}
