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

void main(List<String> args) {
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
