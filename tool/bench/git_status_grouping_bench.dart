// Benchmark: GitChangeGroup.fromEntries and unifiedFromEntries performance.
// Run: dart run tool/bench/git_status_grouping_bench.dart
// Optional real-repo: dart run tool/bench/git_status_grouping_bench.dart --repo <path>

import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:alera/src/shared/infra/git/git_diff_models.dart';

part 'git_status_grouping_bench_entries.dart';

// ---------------------------------------------------------------------------
// Benchmark harness
// ---------------------------------------------------------------------------

const _sizes = <int>[1000, 5000, 20000, 50000];
const _iterations = 5;
const _frameBudgetMs = 16.6;
const _deepUntrackedCount = 7475;

Future<void> main(List<String> args) async {
  print('Git status grouping benchmark\n');

  final fromResults = <int, List<double>>{};
  final unifiedResults = <int, List<double>>{};

  for (final size in _sizes) {
    final entries = generateEntries(size);
    final timed = _timeGrouping(entries);
    fromResults[size] = timed.fromTimes;
    unifiedResults[size] = timed.unifiedTimes;
  }

  _printTable('GitChangeGroup.fromEntries', fromResults);
  _printTable('GitChangeGroup.unifiedFromEntries', unifiedResults);

  final deepEntries = generateDeepUntracked(_deepUntrackedCount);
  final deepTimed = _timeGrouping(deepEntries);
  print(
    '=== Deep untracked $_deepUntrackedCount (synthetic, ~99.99% untracked) ===',
  );
  _printSingle('fromEntries', deepTimed.fromTimes, deepEntries.length);
  _printSingle(
    'unifiedFromEntries',
    deepTimed.unifiedTimes,
    deepEntries.length,
  );
  print('');

  final chunkedResults = <int, _ChunkedGrouping>{};
  final projectionResults = <int, _ChunkedProjection>{};
  for (final size in <int>[7475, 20000, 50000]) {
    final entries = generateEntries(size);
    chunkedResults[size] = await _timeChunkedGrouping(entries);
    projectionResults[size] = await _timeChunkedProjection(entries);
  }
  _printChunkedTable(chunkedResults);
  _printProjectionTable(projectionResults);

  final repo = _repoArg(args);
  if (repo == null) {
    return;
  }
  final repoEntries = loadRepoStatusEntries(repo);
  final untracked = repoEntries
      .where((entry) => entry.area == GitChangeArea.untracked)
      .length;
  print(
    '=== Real repo $repo (${repoEntries.length} entries, '
    '$untracked untracked) ===',
  );
  if (repoEntries.isEmpty) {
    print('  no status entries\n');
    return;
  }
  final repoTimed = _timeGrouping(repoEntries);
  _printSingle('fromEntries', repoTimed.fromTimes, repoEntries.length);
  _printSingle(
    'unifiedFromEntries',
    repoTimed.unifiedTimes,
    repoEntries.length,
  );
  final repoChunked = await _timeChunkedGrouping(repoEntries);
  final repoProjection = await _timeChunkedProjection(repoEntries);
  print('=== Real repo chunked UI-isolate projection ===');
  _printChunkedTable(<int, _ChunkedGrouping>{repoEntries.length: repoChunked});
  _printProjectionTable(<int, _ChunkedProjection>{
    repoEntries.length: repoProjection,
  });
  print('');
}

String? _repoArg(List<String> args) {
  for (var i = 0; i < args.length; i++) {
    if (args[i] == '--repo' && i + 1 < args.length) {
      return args[i + 1];
    }
  }
  return null;
}

class _TimedGrouping {
  const _TimedGrouping({required this.fromTimes, required this.unifiedTimes});

  final List<double> fromTimes;
  final List<double> unifiedTimes;
}

class _ChunkedGrouping {
  const _ChunkedGrouping({
    required this.fromMaxTimes,
    required this.fromWallTimes,
    required this.unifiedMaxTimes,
    required this.unifiedWallTimes,
  });

  final List<double> fromMaxTimes;
  final List<double> fromWallTimes;
  final List<double> unifiedMaxTimes;
  final List<double> unifiedWallTimes;
}

class _ChunkedProjection {
  const _ChunkedProjection({required this.maxTimes, required this.wallTimes});

  final List<double> maxTimes;
  final List<double> wallTimes;
}

_TimedGrouping _timeGrouping(List<GitChangeEntry> entries) {
  final warmupGroups = GitChangeGroup.fromEntries(entries);
  final warmupUnified = GitChangeGroup.unifiedFromEntries(entries);
  var sink = warmupGroups.length + warmupUnified.length;
  for (final group in warmupGroups) {
    sink += group.treeRows.length;
  }
  for (final group in warmupUnified) {
    sink += group.treeRows.length;
  }

  final fromTimes = <double>[];
  var fromSink = 0;
  for (var iter = 0; iter < _iterations; iter++) {
    final stopwatch = Stopwatch()..start();
    final groups = GitChangeGroup.fromEntries(entries);
    stopwatch.stop();
    fromTimes.add(stopwatch.elapsedMicroseconds / 1000.0);
    fromSink += groups.length;
    for (final group in groups) {
      fromSink += group.treeRows.length;
    }
  }

  final unifiedTimes = <double>[];
  var unifiedSink = 0;
  for (var iter = 0; iter < _iterations; iter++) {
    final stopwatch = Stopwatch()..start();
    final groups = GitChangeGroup.unifiedFromEntries(entries);
    stopwatch.stop();
    unifiedTimes.add(stopwatch.elapsedMicroseconds / 1000.0);
    unifiedSink += groups.length;
    for (final group in groups) {
      unifiedSink += group.treeRows.length;
    }
  }

  if (sink + fromSink + unifiedSink < 0) {
    print('(never)');
  }
  return _TimedGrouping(fromTimes: fromTimes, unifiedTimes: unifiedTimes);
}

Future<_ChunkedGrouping> _timeChunkedGrouping(
  List<GitChangeEntry> entries,
) async {
  await GitChangeGroup.fromEntriesChunked(entries);
  await GitChangeGroup.unifiedFromEntriesChunked(entries);

  final fromMaxTimes = <double>[];
  final fromWallTimes = <double>[];
  final unifiedMaxTimes = <double>[];
  final unifiedWallTimes = <double>[];
  var sink = 0;
  for (var iter = 0; iter < _iterations; iter++) {
    var fromMax = 0.0;
    final fromStopwatch = Stopwatch()..start();
    final groups = await GitChangeGroup.fromEntriesChunked(
      entries,
      onChunk: (milliseconds) {
        if (milliseconds > fromMax) {
          fromMax = milliseconds;
        }
      },
    );
    fromStopwatch.stop();
    fromMaxTimes.add(fromMax);
    fromWallTimes.add(fromStopwatch.elapsedMicroseconds / 1000.0);
    sink += groups.length;
    for (final group in groups) {
      sink += group.treeRows.length;
    }

    var unifiedMax = 0.0;
    final unifiedStopwatch = Stopwatch()..start();
    final unifiedGroups = await GitChangeGroup.unifiedFromEntriesChunked(
      entries,
      onChunk: (milliseconds) {
        if (milliseconds > unifiedMax) {
          unifiedMax = milliseconds;
        }
      },
    );
    unifiedStopwatch.stop();
    unifiedMaxTimes.add(unifiedMax);
    unifiedWallTimes.add(unifiedStopwatch.elapsedMicroseconds / 1000.0);
    sink += unifiedGroups.length;
    for (final group in unifiedGroups) {
      sink += group.treeRows.length;
    }
  }
  if (sink < 0) {
    print('(never)');
  }
  return _ChunkedGrouping(
    fromMaxTimes: fromMaxTimes,
    fromWallTimes: fromWallTimes,
    unifiedMaxTimes: unifiedMaxTimes,
    unifiedWallTimes: unifiedWallTimes,
  );
}

Future<_ChunkedProjection> _timeChunkedProjection(
  List<GitChangeEntry> entries,
) async {
  Future<void> project() async {
    final projected = <GitChangeEntry>[];
    for (var index = 0; index < entries.length; index += 1) {
      final entry = entries[index];
      final submodule = entry.submodule;
      projected.add(
        GitChangeEntry(
          path: entry.path,
          oldPath: entry.oldPath,
          area: entry.area,
          status: entry.status,
          added: entry.added,
          removed: entry.removed,
          isBinary: entry.isBinary,
          isLarge: entry.isLarge,
          submodule: submodule == null
              ? null
              : GitSubmoduleStatus(
                  commitChanged: submodule.commitChanged,
                  trackedChanges: submodule.trackedChanges,
                  untrackedChanges: submodule.untrackedChanges,
                  inspectable: submodule.inspectable,
                ),
          submoduleRoot: entry.submoduleRoot,
        ),
      );
      if ((index + 1) % gitStatusWorkChunkSize == 0) {
        await Future.pause();
      }
    }
    await Future.pause();
    List<GitChangeEntry>.unmodifiableOf(projected);
    await Future.pause();
  }

  await project();
  final maxTimes = <double>[];
  final wallTimes = <double>[];
  for (var iter = 0; iter < _iterations; iter++) {
    var maxSegment = 0.0;
    var segmentStopwatch = Stopwatch()..start();
    final wallStopwatch = Stopwatch()..start();
    final projected = <GitChangeEntry>[];
    for (var index = 0; index < entries.length; index += 1) {
      final entry = entries[index];
      final submodule = entry.submodule;
      projected.add(
        GitChangeEntry(
          path: entry.path,
          oldPath: entry.oldPath,
          area: entry.area,
          status: entry.status,
          added: entry.added,
          removed: entry.removed,
          isBinary: entry.isBinary,
          isLarge: entry.isLarge,
          submodule: submodule == null
              ? null
              : GitSubmoduleStatus(
                  commitChanged: submodule.commitChanged,
                  trackedChanges: submodule.trackedChanges,
                  untrackedChanges: submodule.untrackedChanges,
                  inspectable: submodule.inspectable,
                ),
          submoduleRoot: entry.submoduleRoot,
        ),
      );
      if ((index + 1) % gitStatusWorkChunkSize == 0) {
        segmentStopwatch.stop();
        final milliseconds = segmentStopwatch.elapsedMicroseconds / 1000.0;
        if (milliseconds > maxSegment) {
          maxSegment = milliseconds;
        }
        await Future.pause();
        segmentStopwatch = Stopwatch()..start();
      }
    }
    segmentStopwatch.stop();
    final milliseconds = segmentStopwatch.elapsedMicroseconds / 1000.0;
    if (milliseconds > maxSegment) {
      maxSegment = milliseconds;
    }
    await Future.pause();
    segmentStopwatch = Stopwatch()..start();
    List<GitChangeEntry>.unmodifiableOf(projected);
    segmentStopwatch.stop();
    final copyMilliseconds = segmentStopwatch.elapsedMicroseconds / 1000.0;
    if (copyMilliseconds > maxSegment) {
      maxSegment = copyMilliseconds;
    }
    wallStopwatch.stop();
    maxTimes.add(maxSegment);
    wallTimes.add(wallStopwatch.elapsedMicroseconds / 1000.0);
  }
  return _ChunkedProjection(maxTimes: maxTimes, wallTimes: wallTimes);
}

double _maxOrZero(List<double> values) {
  if (values.isEmpty) {
    return 0;
  }
  var maximum = values.first;
  for (final value in values.skip(1)) {
    if (value > maximum) {
      maximum = value;
    }
  }
  return maximum;
}

void _printSingle(String title, List<double> times, int size) {
  final med = _median(times);
  final row = StringBuffer();
  row.write(size.toString().padRight(8));
  for (final time in times) {
    row.write(_fmtMs(time).padLeft(10));
  }
  row.write(_fmtMs(med).padLeft(12));
  row.write('  $title');
  print(row);
  final verdict = med > _frameBudgetMs
      ? 'EXCEEDS one frame budget (${_frameBudgetMs}ms)'
      : 'fits within one frame budget (${_frameBudgetMs}ms)';
  print('  $size entries: median ${med.toStringAsFixed(2)}ms -> $verdict');
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

double _median(List<double> values) {
  final sorted = List<double>.of(values)..sort();
  final mid = sorted.length ~/ 2;
  if (sorted.length.isOdd) return sorted[mid];
  return (sorted[mid - 1] + sorted[mid]) / 2.0;
}

String _fmtMs(double ms) => ms.toStringAsFixed(2).padLeft(8);

void _printTable(String title, Map<int, List<double>> results) {
  print('=== $title ===');
  // Header
  final header = StringBuffer();
  header.write('entries'.padRight(8));
  for (var i = 1; i <= _iterations; i++) {
    header.write('  run$i(ms)'.padLeft(10));
  }
  header.write('  median(ms)'.padLeft(12));
  print(header.toString());
  print('-' * (8 + _iterations * 10 + 12));

  for (final size in _sizes) {
    final times = results[size]!;
    final med = _median(times);
    final row = StringBuffer();
    row.write(size.toString().padRight(8));
    for (final t in times) {
      row.write(_fmtMs(t).padLeft(10));
    }
    row.write(_fmtMs(med).padLeft(12));
    print(row.toString());
  }

  print('');

  // Verdicts per size.
  for (final size in _sizes) {
    final med = _median(results[size]!);
    final verdict = med > _frameBudgetMs
        ? 'EXCEEDS one frame budget (${_frameBudgetMs}ms)'
        : 'fits within one frame budget (${_frameBudgetMs}ms)';
    print('  $size entries: median ${med.toStringAsFixed(2)}ms -> $verdict');
  }
  print('');
}

void _printChunkedTable(Map<int, _ChunkedGrouping> results) {
  print('=== Chunked UI-isolate projection ===');
  print(
    'entries   from max med  from max obs  from wall med  '
    'unified max med  unified max obs  unified wall med',
  );
  print('-' * 95);
  for (final entry in results.entries) {
    final value = entry.value;
    print(
      '${entry.key.toString().padRight(8)}'
      '${_fmtMs(_median(value.fromMaxTimes)).padLeft(14)}'
      '${_fmtMs(_maxOrZero(value.fromMaxTimes)).padLeft(14)}'
      '${_fmtMs(_median(value.fromWallTimes)).padLeft(15)}'
      '${_fmtMs(_median(value.unifiedMaxTimes)).padLeft(17)}'
      '${_fmtMs(_maxOrZero(value.unifiedMaxTimes)).padLeft(16)}'
      '${_fmtMs(_median(value.unifiedWallTimes)).padLeft(17)}',
    );
  }
  print('  max segment is the longest synchronous interval between yields.');
  print('');
}

void _printProjectionTable(Map<int, _ChunkedProjection> results) {
  print('=== Chunked status entry projection (translate proxy) ===');
  print('entries   max med(ms)  max obs(ms)  wall median(ms)');
  print('-' * 56);
  for (final entry in results.entries) {
    final value = entry.value;
    print(
      '${entry.key.toString().padRight(8)}'
      '${_fmtMs(_median(value.maxTimes)).padLeft(14)}'
      '${_fmtMs(_maxOrZero(value.maxTimes)).padLeft(13)}'
      '${_fmtMs(_median(value.wallTimes)).padLeft(17)}',
    );
  }
  print('');
}
