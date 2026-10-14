import 'dart:typed_data';

import 'package:alera/src/shared/infra/files/path_identity.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';

typedef BranchCompareFileKey = (
  String repositoryPath,
  String commitOid,
  String? parentOid,
  String? filePath,
  String? oldPath,
);

/// Retains immutable Git queries for one open branch comparison tab.
final class WorkspaceGitBranchCompareCache {
  (String, String, String)? _snapshotKey;
  Future<GitCommitCompareResult>? _snapshot;
  int _generation = 0;
  final _diffs =
      _BranchCompareQueryCache<
        (BranchCompareFileKey, GitDiffWhitespaceMode),
        GitDiffResult
      >(maxEntries: 16, maxBytes: 8 * 1024 * 1024);
  final _blobs =
      _BranchCompareQueryCache<(BranchCompareFileKey, bool), Uint8List?>(
        maxEntries: 32,
        maxBytes: 8 * 1024 * 1024,
      );

  String? selectedPath;

  Future<GitCommitCompareResult> snapshot({
    required String path,
    required String baseOid,
    required String headOid,
    required Future<GitCommitCompareResult> Function() load,
  }) {
    _bind(path, baseOid, headOid);
    return _snapshot ??= _loadSnapshot(load);
  }

  Future<GitCommitCompareResult> _loadSnapshot(
    Future<GitCommitCompareResult> Function() load,
  ) async {
    final generation = _generation;
    try {
      final result = await Future<GitCommitCompareResult>.sync(load);
      if (result.summary.status != GitCommitCompareStatus.ready) {
        throw GitInternalException(
          result.summary.errorMessage ?? 'Could not compare branches.',
        );
      }
      return result;
    } catch (_) {
      if (_generation == generation) _snapshot = null;
      rethrow;
    }
  }

  void acceptSnapshot(String path, GitCommitCompareResult result) {
    _bind(path, result.summary.parentOid!, result.summary.commitOid);
    ++_generation;
    _snapshot = Future.value(result);
  }

  void _bind(String path, String baseOid, String headOid) {
    final key = (comparablePath(path), baseOid, headOid);
    if (_snapshotKey == key) return;
    clear();
    _snapshotKey = key;
  }

  Future<GitDiffResult> diff({
    required BranchCompareFileKey key,
    required GitDiffWhitespaceMode whitespaceMode,
    required Future<GitDiffResult> Function() load,
  }) => _diffs.load((_normalizedKey(key), whitespaceMode), load, (result) {
    var bytes = 0;
    for (final file in result.files) {
      for (final line in file.lines) {
        bytes += 128 + line.text.length * 2;
        if (bytes > 8 * 1024 * 1024) return bytes;
      }
    }
    return bytes;
  });

  Future<Uint8List?> blob({
    required BranchCompareFileKey key,
    required bool oldSide,
    required Future<Uint8List?> Function() load,
  }) => _blobs.load(
    (_normalizedKey(key), oldSide),
    load,
    (bytes) => bytes?.lengthInBytes ?? 0,
  );

  BranchCompareFileKey _normalizedKey(BranchCompareFileKey key) =>
      (comparablePath(key.$1), key.$2, key.$3, key.$4, key.$5);

  void clear() {
    ++_generation;
    _snapshotKey = null;
    _snapshot = null;
    _diffs.clear();
    _blobs.clear();
  }
}

final class _BranchCompareQueryCache<K, V> {
  _BranchCompareQueryCache({required this.maxEntries, required this.maxBytes});

  final int maxEntries;
  final int maxBytes;
  final _entries = <K, Future<V>>{};
  final _sizes = <K, int>{};
  int _bytes = 0;

  Future<V> load(K key, Future<V> Function() load, int Function(V) size) {
    final cached = _entries.remove(key);
    if (cached != null) {
      _entries[key] = cached;
      return cached;
    }
    late final Future<V> future;
    future = Future<V>.sync(load).then(
      (value) {
        if (identical(_entries[key], future)) {
          final bytes = size(value);
          if (bytes > maxBytes) {
            _remove(key);
          } else {
            _sizes[key] = bytes;
            _bytes += bytes;
            _trim();
          }
        }
        return value;
      },
      onError: (Object error, StackTrace stack) {
        if (identical(_entries[key], future)) _remove(key);
        Error.throwWithStackTrace(error, stack);
      },
    );
    _entries[key] = future;
    _trim();
    return future;
  }

  void _trim() {
    while (_entries.length > maxEntries || _bytes > maxBytes) {
      _remove(_entries.keys.first);
    }
  }

  void _remove(K key) {
    _entries.remove(key);
    _bytes -= _sizes.remove(key) ?? 0;
  }

  void clear() {
    _entries.clear();
    _sizes.clear();
    _bytes = 0;
  }
}
