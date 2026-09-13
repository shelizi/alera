// ignore_for_file: avoid_print

// Benchmark: GitChangeGroup.fromEntries and unifiedFromEntries performance
// against real-repository git status entries.
// Run: dart run tool/bench/git_status_real_repo_bench.dart [repoPath]

import 'dart:convert';
import 'dart:io';

import 'package:alera/src/shared/infra/git/git_diff_models.dart';
part 'git_status_real_repo_loader.dart';
part 'git_status_bench_report.dart';

const _defaultRepoPath = r'E:\Dropbox\work\coding-tools-mcp';
const _iterations = 5;
const _frameBudgetMs = 16.6;

// Synthetic reference numbers from git_status_grouping_bench.dart (fromEntries):
// 1000 ≈ 4ms, 5000 ≈ 16ms, 20000 ≈ 77ms, 50000 ≈ 242ms.
const _syntheticRefs = <int, double>{
  1000: 4.0,
  5000: 16.0,
  20000: 77.0,
  50000: 242.0,
};

void main(List<String> args) async {
  final targetRepo = args.isNotEmpty ? args[0] : _defaultRepoPath;

  print('Real-repo git status grouping benchmark\n');

  final status = await collectRealEntries(targetRepo);
  final realEntries = status.entries;

  var stagedCount = 0;
  var unstagedCount = 0;
  var untrackedCount = 0;
  final statusCounts = <String, int>{
    'M': 0,
    'A': 0,
    'D': 0,
    'R': 0,
    'C': 0,
    'U': 0,
  };

  for (final e in realEntries) {
    switch (e.area) {
      case GitChangeArea.staged:
        stagedCount++;
      case GitChangeArea.unstaged:
        unstagedCount++;
      case GitChangeArea.untracked:
        untrackedCount++;
    }
    final key = switch (e.status) {
      GitChangeStatus.modified => 'M',
      GitChangeStatus.added => 'A',
      GitChangeStatus.deleted => 'D',
      GitChangeStatus.renamed => 'R',
      GitChangeStatus.copied => 'C',
      GitChangeStatus.untracked => 'U',
    };
    statusCounts[key] = (statusCounts[key] ?? 0) + 1;
  }

  print('Repo path: ${status.repoPath}');
  print('Porcelain version: ${status.porcelainVersion}');
  print('Total parsed entries: ${realEntries.length}');
  print(
    'Distribution: staged=$stagedCount, unstaged=$unstagedCount, untracked=$untrackedCount',
  );
  print(
    'Status breakdown: M=${statusCounts['M']}, A=${statusCounts['A']}, D=${statusCounts['D']}, R=${statusCounts['R']}, C=${statusCounts['C']}, U=${statusCounts['U']}\n',
  );

  final entries20k = scaleEntries(realEntries, 20000);
  final entries50k = scaleEntries(realEntries, 50000);

  final sizes = <int>[realEntries.length, 20000, 50000];
  final datasetMap = <int, List<GitChangeEntry>>{
    realEntries.length: realEntries,
    20000: entries20k,
    50000: entries50k,
  };

  final fromResults = <int, List<double>>{};
  final unifiedResults = <int, List<double>>{};

  for (final size in sizes) {
    final entries = datasetMap[size]!;

    // Warmup - excluded from timing.
    final warmupGroups = GitChangeGroup.fromEntries(entries);
    final warmupUnified = GitChangeGroup.unifiedFromEntries(entries);
    var sink = warmupGroups.length + warmupUnified.length;
    for (final g in warmupGroups) {
      sink += g.treeRows.length;
    }
    for (final g in warmupUnified) {
      sink += g.treeRows.length;
    }

    final fromTimes = <double>[];
    var fromSink = 0;
    for (var iter = 0; iter < _iterations; iter++) {
      final sw = Stopwatch()..start();
      final groups = GitChangeGroup.fromEntries(entries);
      sw.stop();
      fromTimes.add(sw.elapsedMicroseconds / 1000.0);
      fromSink += groups.length;
      for (final g in groups) {
        fromSink += g.treeRows.length;
      }
    }
    fromResults[size] = fromTimes;

    final unifiedTimes = <double>[];
    var unifiedSink = 0;
    for (var iter = 0; iter < _iterations; iter++) {
      final sw = Stopwatch()..start();
      final groups = GitChangeGroup.unifiedFromEntries(entries);
      sw.stop();
      unifiedTimes.add(sw.elapsedMicroseconds / 1000.0);
      unifiedSink += groups.length;
      for (final g in groups) {
        unifiedSink += g.treeRows.length;
      }
    }
    unifiedResults[size] = unifiedTimes;

    if (sink + fromSink + unifiedSink < 0) print('(never)');
  }

  _printTable(
    'GitChangeGroup.fromEntries',
    fromResults,
    sizes,
    realEntries.length,
  );
  _printTable(
    'GitChangeGroup.unifiedFromEntries',
    unifiedResults,
    sizes,
    realEntries.length,
  );
  _printComparisonTable(fromResults, unifiedResults, sizes, realEntries.length);
}
