part of 'git_diff_models.dart';

/// Bounds the synchronous work between event-loop yields while projecting a
/// large status result.
const gitStatusWorkChunkSize = 16;

class _GitStatusChunker {
  _GitStatusChunker(this._onChunk, this._chunkSize) {
    if (_onChunk != null) {
      _stopwatch = Stopwatch()..start();
    }
  }

  final void Function(double milliseconds)? _onChunk;
  final int _chunkSize;
  int _operationCount = 0;
  Stopwatch? _stopwatch;

  Future<void> checkpoint() async {
    _operationCount += 1;
    if (_operationCount % _chunkSize == 0) {
      await pause();
    }
  }

  Future<void> pause() async {
    _record();
    await Future.pause();
    final stopwatch = _stopwatch;
    if (stopwatch != null) {
      stopwatch
        ..reset()
        ..start();
    }
  }

  void finish() {
    _record();
  }

  void _record() {
    final stopwatch = _stopwatch;
    final onChunk = _onChunk;
    if (stopwatch == null || onChunk == null) {
      return;
    }
    stopwatch.stop();
    onChunk(stopwatch.elapsedMicroseconds / 1000.0);
  }
}

Future<void> _sortListChunked<T>(
  List<T> values,
  int Function(T left, T right) compare,
  int chunkSize,
  _GitStatusChunker chunker,
) async {
  if (values.length < 2) {
    return;
  }

  final runSize = math.min(chunkSize, values.length);
  for (var start = 0; start < values.length; start += runSize) {
    final end = math.min(start + runSize, values.length);
    final run = values.sublist(start, end)..sort(compare);
    values.setRange(start, end, run);
    if (end < values.length) {
      await chunker.pause();
    }
  }
  if (runSize == values.length) {
    return;
  }

  var source = values;
  var target = List<T>.filled(values.length, values.first);
  for (var width = runSize; width < values.length; width *= 2) {
    for (var start = 0; start < values.length; start += width * 2) {
      final middle = math.min(start + width, values.length);
      final end = math.min(start + width * 2, values.length);
      var left = start;
      var right = middle;
      var destination = start;

      while (left < middle && right < end) {
        final leftValue = source[left];
        final rightValue = source[right];
        if (compare(leftValue, rightValue) <= 0) {
          target[destination] = leftValue;
          left += 1;
        } else {
          target[destination] = rightValue;
          right += 1;
        }
        destination += 1;
        await chunker.checkpoint();
      }
      while (left < middle) {
        target[destination] = source[left];
        left += 1;
        destination += 1;
        await chunker.checkpoint();
      }
      while (right < end) {
        target[destination] = source[right];
        right += 1;
        destination += 1;
        await chunker.checkpoint();
      }
    }

    final previousSource = source;
    source = target;
    target = previousSource;
    if (width > values.length ~/ 2) {
      break;
    }
  }

  if (!identical(source, values)) {
    for (var index = 0; index < values.length; index += 1) {
      values[index] = source[index];
      await chunker.checkpoint();
    }
  }
}

class _GitChangeTreeNode({
  required final String name,
  required final String path,
  required final int depth,
}) {
  List<_GitChangeTreeNode>? subdirectories;
  List<GitChangeTreeRow>? fileRows;
  int fileCount = 0;

  void addSubdirectory(_GitChangeTreeNode node) {
    (subdirectories ??= <_GitChangeTreeNode>[]).add(node);
  }

  void addFileRow(GitChangeTreeRow row) {
    (fileRows ??= <GitChangeTreeRow>[]).add(row);
  }

  void finalizeTree() {
    final subs = subdirectories;
    if (subs != null && subs.length > 1) {
      subs.sort((a, b) => a.name.compareTo(b.name));
    }
    var count = fileRows?.length ?? 0;
    if (subs != null) {
      for (var i = 0; i < subs.length; i++) {
        final sub = subs[i];
        sub.finalizeTree();
        count += sub.fileCount;
      }
    }
    fileCount = count;
  }

  Future<void> finalizeTreeChunked(
    int chunkSize,
    _GitStatusChunker chunker,
  ) async {
    final subs = subdirectories;
    if (subs != null && subs.length > 1) {
      await _sortListChunked(
        subs,
        (a, b) => a.name.compareTo(b.name),
        chunkSize,
        chunker,
      );
    }
    var count = fileRows?.length ?? 0;
    if (subs != null) {
      for (var index = 0; index < subs.length; index += 1) {
        final sub = subs[index];
        await sub.finalizeTreeChunked(chunkSize, chunker);
        count += sub.fileCount;
        await chunker.checkpoint();
      }
    }
    fileCount = count;
  }

  void appendRows(List<GitChangeTreeRow> rows) {
    rows.add(
      GitChangeTreeRow(
        kind: GitChangeTreeRowKind.directory,
        name: name,
        path: path,
        depth: depth,
        fileCount: fileCount,
        entry: null,
      ),
    );
    final subs = subdirectories;
    if (subs != null) {
      for (var i = 0; i < subs.length; i++) {
        subs[i].appendRows(rows);
      }
    }
    final files = fileRows;
    if (files != null) {
      for (var i = 0; i < files.length; i++) {
        rows.add(files[i]);
      }
    }
  }

  Future<void> appendRowsChunked(
    List<GitChangeTreeRow> rows,
    int chunkSize,
    _GitStatusChunker chunker,
  ) async {
    rows.add(
      GitChangeTreeRow(
        kind: GitChangeTreeRowKind.directory,
        name: name,
        path: path,
        depth: depth,
        fileCount: fileCount,
        entry: null,
      ),
    );
    await chunker.checkpoint();

    final subs = subdirectories;
    if (subs != null) {
      for (final sub in subs) {
        await sub.appendRowsChunked(rows, chunkSize, chunker);
      }
    }
    final files = fileRows;
    if (files != null) {
      for (final file in files) {
        rows.add(file);
        await chunker.checkpoint();
      }
    }
  }
}
