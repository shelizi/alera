// ignore_for_file: avoid_print

// Benchmark: retained workspace-search grouping/sorting + visible row projection.
// Run: dart run tool/bench/workspace_search_projection_bench.dart

import 'dart:math';

import 'package:alera/src/features/workbench/application/workspace_search_projection.dart';
import 'package:alera/src/rust/api/workspace_search.dart' as native;
import 'package:path/path.dart' as p;

const _sizes = <int>[1000, 2000, 10000, 50000];
const _iterations = 5;

void main() {
  print('Workspace search projection benchmark');
  print('Synthetic matches include one match per file.\n');

  for (final size in _sizes) {
    final result = _makeResult(size);
    final legacyExpandedTimes = <double>[];
    final retainedColdTimes = <double>[];
    final retainedExpandedTimes = <double>[];
    final retainedCollapsedTimes = <double>[];
    var sink = 0;

    for (var i = 0; i < _iterations; i++) {
      final legacy = Stopwatch()..start();
      sink += _legacyTreeRowCount(result, const <String>{});
      legacy.stop();
      legacyExpandedTimes.add(legacy.elapsedMicroseconds / 1000.0);

      final cold = Stopwatch()..start();
      final projection = WorkspaceSearchProjection(result);
      final coldRows = projection.rows(
        collapsedResultNodeKeys: const <String>{},
        viewAsTree: true,
      );
      cold.stop();
      retainedColdTimes.add(cold.elapsedMicroseconds / 1000.0);
      sink += coldRows.length;

      final expand = Stopwatch()..start();
      final expanded = projection.rows(
        collapsedResultNodeKeys: const <String>{},
        viewAsTree: true,
      );
      expand.stop();
      retainedExpandedTimes.add(expand.elapsedMicroseconds / 1000.0);
      sink += expanded.length;

      final keys = projection.collapsibleNodeKeys(viewAsTree: true);
      final collapse = Stopwatch()..start();
      final collapsed = projection.rows(
        collapsedResultNodeKeys: keys,
        viewAsTree: true,
      );
      collapse.stop();
      retainedCollapsedTimes.add(collapse.elapsedMicroseconds / 1000.0);
      sink += collapsed.length;
    }

    print('$size matches');
    _printMetric('legacy expanded rebuild', legacyExpandedTimes);
    _printMetric('retained cold expanded', retainedColdTimes);
    _printMetric('retained warm expanded', retainedExpandedTimes);
    _printMetric('retained all-collapsed', retainedCollapsedTimes);
    print('');

    if (sink < 0) {
      print('unreachable: $sink');
    }
  }
}

void _printMetric(String label, List<double> samples) {
  final sorted = samples.toList()..sort();
  final median = sorted[sorted.length ~/ 2];
  final p95 = sorted[min(sorted.length - 1, (sorted.length * .95).ceil() - 1)];
  print(
    '  ${label.padRight(22)} median=${median.toStringAsFixed(3)} ms '
    'p95=${p95.toStringAsFixed(3)} ms',
  );
}

final p.Context _pathContext = p.Context(style: p.Style.posix);

int _legacyTreeRowCount(
  native.WorkspaceSearchResult result,
  Set<String> collapsedResultNodeKeys,
) {
  final root = _LegacyDirectory('', '');
  for (final file in result.files) {
    final segments = _pathSegments(file.relativePath);
    if (segments.length <= 1) {
      root.files.add(file);
      continue;
    }
    var directory = root;
    for (var index = 0; index < segments.length - 1; index += 1) {
      final name = segments[index];
      final path = directory.path.isEmpty
          ? name
          : _pathContext.join(directory.path, name);
      directory = directory.directories.putIfAbsent(
        name,
        () => _LegacyDirectory(name, path),
      );
    }
    directory.files.add(file);
  }
  return _legacyAppendRowCount(root, collapsedResultNodeKeys);
}

int _legacyAppendRowCount(
  _LegacyDirectory directory,
  Set<String> collapsedResultNodeKeys,
) {
  var count = 0;
  final children = directory.directories.values.toList(growable: false)
    ..sort((a, b) => a.name.compareTo(b.name));
  for (final child in children) {
    count += 1;
    if (!collapsedResultNodeKeys.contains(
      workspaceSearchDirectoryNodeKey(child.path),
    )) {
      count += _legacyAppendRowCount(child, collapsedResultNodeKeys);
    }
  }
  final files = directory.files.toList(growable: false)
    ..sort((a, b) => a.relativePath.compareTo(b.relativePath));
  for (final file in files) {
    count += 1;
    if (!collapsedResultNodeKeys.contains(
      workspaceSearchFileNodeKey(file.relativePath),
    )) {
      count += file.matches.length;
    }
  }
  return count;
}

List<String> _pathSegments(String relativePath) {
  return _pathContext
      .split(relativePath.replaceAll('\\', '/'))
      .where((segment) => segment.isNotEmpty)
      .toList(growable: false);
}

final class _LegacyDirectory {
  _LegacyDirectory(this.name, this.path);

  final String name;
  final String path;
  final Map<String, _LegacyDirectory> directories =
      <String, _LegacyDirectory>{};
  final List<native.WorkspaceSearchFileResult> files =
      <native.WorkspaceSearchFileResult>[];
}

native.WorkspaceSearchResult _makeResult(int count) {
  final files = <native.WorkspaceSearchFileResult>[];
  for (var i = 0; i < count; i++) {
    final bucket = i % 64;
    final feature = (i ~/ 64) % 64;
    final relativePath = 'src/feature_$feature/group_$bucket/file_$i.dart';
    files.add(
      native.WorkspaceSearchFileResult(
        relativePath: relativePath,
        contentToken: 'token-$i',
        matches: <native.WorkspaceSearchMatch>[
          native.WorkspaceSearchMatch(
            id: '$relativePath:1:1:0',
            line: 1,
            column: 1,
            matchLength: 6,
            lineContent: 'needle ${i % 10}',
          ),
        ],
      ),
    );
  }
  return native.WorkspaceSearchResult(
    totalMatches: count,
    truncated: false,
    files: files,
  );
}
