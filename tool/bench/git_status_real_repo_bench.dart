// ignore_for_file: avoid_print

// Benchmark: GitChangeGroup.fromEntries and unifiedFromEntries performance
// against real-repository git status entries.
// Run: dart run tool/bench/git_status_real_repo_bench.dart [repoPath]

import 'dart:convert';
import 'dart:io';

import 'package:alera/src/shared/infra/git/git_diff_models.dart';

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

int _findNthSpace(String str, int n) {
  var count = 0;
  for (var i = 0; i < str.length; i++) {
    if (str.codeUnitAt(i) == 32) {
      count++;
      if (count == n) return i;
    }
  }
  return -1;
}

GitChangeStatus? _mapStatusChar(String ch) {
  return switch (ch) {
    'M' => GitChangeStatus.modified,
    'A' => GitChangeStatus.added,
    'D' => GitChangeStatus.deleted,
    'R' => GitChangeStatus.renamed,
    'C' => GitChangeStatus.copied,
    'T' => GitChangeStatus.modified,
    'U' => GitChangeStatus.modified,
    '?' => GitChangeStatus.untracked,
    _ => null,
  };
}

class ParsedGitStatus {
  final String repoPath;
  final String porcelainVersion;
  final List<GitChangeEntry> entries;

  const ParsedGitStatus({
    required this.repoPath,
    required this.porcelainVersion,
    required this.entries,
  });
}

Future<ParsedGitStatus> collectRealEntries(String repoPath) async {
  var porcelainVersion = 'porcelain v2';
  var res = await Process.run(
    'git',
    ['-C', repoPath, 'status', '--porcelain=v2', '-z', '--untracked-files=all'],
    stdoutEncoding: null,
  );

  var isV2 = true;
  if (res.exitCode != 0) {
    porcelainVersion = 'porcelain v1';
    isV2 = false;
    res = await Process.run(
      'git',
      ['-C', repoPath, 'status', '--porcelain', '-z', '--untracked-files=all'],
      stdoutEncoding: null,
    );
    if (res.exitCode != 0) {
      final err = res.stderr != null
          ? utf8.decode(res.stderr as List<int>, allowMalformed: true)
          : '';
      throw ProcessException(
        'git',
        ['status'],
        'git status failed with exit code ${res.exitCode}: $err',
        res.exitCode,
      );
    }
  }

  final bytes = res.stdout as List<int>;
  final records = <String>[];
  var start = 0;
  for (var i = 0; i < bytes.length; i++) {
    if (bytes[i] == 0) {
      records.add(utf8.decode(bytes.sublist(start, i), allowMalformed: true));
      start = i + 1;
    }
  }
  if (start < bytes.length) {
    final tail = utf8.decode(bytes.sublist(start), allowMalformed: true);
    if (tail.isNotEmpty) records.add(tail);
  }

  final entries = <GitChangeEntry>[];

  if (isV2) {
    for (var i = 0; i < records.length; i++) {
      final record = records[i];
      if (record.isEmpty || record.startsWith('#')) continue;

      if (record.startsWith('? ')) {
        final path = record.substring(2);
        entries.add(GitChangeEntry(
          path: path,
          area: GitChangeArea.untracked,
          status: GitChangeStatus.untracked,
        ));
      } else if (record.startsWith('! ')) {
        continue;
      } else if (record.startsWith('1 ')) {
        if (record.length < 4) continue;
        final xChar = record[2];
        final yChar = record[3];
        final pathIdx = _findNthSpace(record, 8);
        if (pathIdx == -1 || pathIdx + 1 >= record.length) continue;
        final path = record.substring(pathIdx + 1);

        if (xChar != '.') {
          final s = _mapStatusChar(xChar);
          if (s != null) {
            entries.add(GitChangeEntry(
              path: path,
              area: GitChangeArea.staged,
              status: s,
            ));
          }
        }
        if (yChar != '.') {
          final s = _mapStatusChar(yChar);
          if (s != null) {
            entries.add(GitChangeEntry(
              path: path,
              area: GitChangeArea.unstaged,
              status: s,
            ));
          }
        }
      } else if (record.startsWith('2 ')) {
        if (record.length < 4) continue;
        final xChar = record[2];
        final yChar = record[3];
        final pathIdx = _findNthSpace(record, 9);
        if (pathIdx == -1 || pathIdx + 1 >= record.length) continue;
        final path = record.substring(pathIdx + 1);
        final origPath = i + 1 < records.length ? records[++i] : null;

        if (xChar != '.') {
          final s = _mapStatusChar(xChar);
          if (s != null) {
            entries.add(GitChangeEntry(
              path: path,
              area: GitChangeArea.staged,
              status: s,
              oldPath: origPath,
            ));
          }
        }
        if (yChar != '.') {
          final s = _mapStatusChar(yChar);
          if (s != null) {
            entries.add(GitChangeEntry(
              path: path,
              area: GitChangeArea.unstaged,
              status: s,
              oldPath: origPath,
            ));
          }
        }
      } else if (record.startsWith('u ')) {
        final pathIdx = _findNthSpace(record, 10);
        if (pathIdx == -1 || pathIdx + 1 >= record.length) continue;
        final path = record.substring(pathIdx + 1);
        entries.add(GitChangeEntry(
          path: path,
          area: GitChangeArea.unstaged,
          status: GitChangeStatus.modified,
        ));
      }
    }
  } else {
    // v1 fallback
    for (var i = 0; i < records.length; i++) {
      final record = records[i];
      if (record.isEmpty || record.startsWith('#')) continue;
      if (record.startsWith('?? ')) {
        entries.add(GitChangeEntry(
          path: record.substring(3),
          area: GitChangeArea.untracked,
          status: GitChangeStatus.untracked,
        ));
      } else if (record.startsWith('!! ')) {
        continue;
      } else if (record.length >= 4) {
        final xChar = record[0];
        final yChar = record[1];
        final path = record.substring(3);
        final isRename =
            xChar == 'R' || xChar == 'C' || yChar == 'R' || yChar == 'C';
        final origPath =
            isRename && i + 1 < records.length ? records[++i] : null;

        if (xChar != ' ' && xChar != '.') {
          final s = _mapStatusChar(xChar);
          if (s != null) {
            entries.add(GitChangeEntry(
              path: path,
              area: GitChangeArea.staged,
              status: s,
              oldPath: origPath,
            ));
          }
        }
        if (yChar != ' ' && yChar != '.') {
          final s = _mapStatusChar(yChar);
          if (s != null) {
            entries.add(GitChangeEntry(
              path: path,
              area: GitChangeArea.unstaged,
              status: s,
              oldPath: origPath,
            ));
          }
        }
      }
    }
  }

  return ParsedGitStatus(
    repoPath: repoPath,
    porcelainVersion: porcelainVersion,
    entries: entries,
  );
}

List<GitChangeEntry> scaleEntries(
  List<GitChangeEntry> realEntries,
  int targetCount,
) {
  if (realEntries.isEmpty) return const <GitChangeEntry>[];
  if (realEntries.length >= targetCount) {
    return realEntries.sublist(0, targetCount);
  }

  final staged = realEntries
      .where((e) => e.area == GitChangeArea.staged)
      .toList(growable: false);
  final unstaged = realEntries
      .where((e) => e.area == GitChangeArea.unstaged)
      .toList(growable: false);
  final untracked = realEntries
      .where((e) => e.area == GitChangeArea.untracked)
      .toList(growable: false);

  final areaGroups = <List<GitChangeEntry>>[staged, unstaged, untracked]
      .where((group) => group.isNotEmpty)
      .toList();

  final counts = <int>[];
  var allocated = 0;
  for (final group in areaGroups) {
    final count = (targetCount * group.length / realEntries.length).round();
    counts.add(count);
    allocated += count;
  }

  var diff = targetCount - allocated;
  if (diff != 0 && counts.isNotEmpty) {
    var maxIdx = 0;
    for (var i = 1; i < counts.length; i++) {
      if (counts[i] > counts[maxIdx]) maxIdx = i;
    }
    counts[maxIdx] += diff;
  }

  final result = <GitChangeEntry>[];
  for (var g = 0; g < areaGroups.length; g++) {
    final group = areaGroups[g];
    final target = counts[g];
    for (var i = 0; i < target; i++) {
      final src = group[i % group.length];
      final copyIdx = i ~/ group.length;
      final path = copyIdx == 0
          ? src.path
          : '_rep${copyIdx.toString().padLeft(2, '0')}/${src.path}';
      final oldPath = src.oldPath != null
          ? (copyIdx == 0
              ? src.oldPath
              : '_rep${copyIdx.toString().padLeft(2, '0')}/${src.oldPath}')
          : null;
      result.add(GitChangeEntry(
        path: path,
        area: src.area,
        status: src.status,
        oldPath: oldPath,
      ));
    }
  }

  return result;
}

double _median(List<double> values) {
  final sorted = List<double>.of(values)..sort();
  final mid = sorted.length ~/ 2;
  if (sorted.length.isOdd) return sorted[mid];
  return (sorted[mid - 1] + sorted[mid]) / 2.0;
}

String _fmtMs(double ms) => ms.toStringAsFixed(2).padLeft(8);

void _printTable(
  String title,
  Map<int, List<double>> results,
  List<int> sizes,
  int realSize,
) {
  print('=== $title ===');
  final header = StringBuffer();
  header.write('entries'.padRight(16));
  for (var i = 1; i <= _iterations; i++) {
    header.write('  run$i(ms)'.padLeft(10));
  }
  header.write('  median(ms)'.padLeft(12));
  print(header.toString());
  print('-' * (16 + _iterations * 10 + 12));

  for (final size in sizes) {
    final times = results[size]!;
    final med = _median(times);
    final label = size == realSize ? '$size (real)' : '$size (scaled)';
    final row = StringBuffer();
    row.write(label.padRight(16));
    for (final t in times) {
      row.write(_fmtMs(t).padLeft(10));
    }
    row.write(_fmtMs(med).padLeft(12));
    print(row.toString());
  }
  print('');

  for (final size in sizes) {
    final med = _median(results[size]!);
    final label =
        size == realSize ? '$size entries (real)' : '$size entries (scaled)';
    final verdict = med > _frameBudgetMs
        ? 'EXCEEDS one frame budget (${_frameBudgetMs}ms)'
        : 'fits within one frame budget (${_frameBudgetMs}ms)';
    print('  $label: median ${med.toStringAsFixed(2)}ms -> $verdict');
  }
  print('');
}

void _printComparisonTable(
  Map<int, List<double>> fromResults,
  Map<int, List<double>> unifiedResults,
  List<int> sizes,
  int realSize,
) {
  print('=== Comparison with Synthetic Reference (fromEntries) ===');
  final header = StringBuffer();
  header.write('entries'.padRight(16));
  header.write('  real(ms)'.padLeft(10));
  header.write('  unified(ms)'.padLeft(13));
  header.write('  synth ref(ms)'.padLeft(15));
  header.write('  ratio'.padLeft(9));
  print(header.toString());
  print('-' * (16 + 10 + 13 + 15 + 9));

  for (final size in sizes) {
    final fromMed = _median(fromResults[size]!);
    final unifiedMed = _median(unifiedResults[size]!);
    final label = size == realSize ? '$size (real)' : '$size (scaled)';
    final ref = _syntheticRefs[size];

    final row = StringBuffer();
    row.write(label.padRight(16));
    row.write(_fmtMs(fromMed).padLeft(10));
    row.write(_fmtMs(unifiedMed).padLeft(13));
    if (ref != null) {
      row.write(_fmtMs(ref).padLeft(15));
      final ratio = '${(fromMed / ref).toStringAsFixed(2)}x';
      row.write(ratio.padLeft(9));
    } else {
      row.write('n/a'.padLeft(15));
      row.write('-'.padLeft(9));
    }
    print(row.toString());
  }
  print('');
}

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
  _printComparisonTable(
    fromResults,
    unifiedResults,
    sizes,
    realEntries.length,
  );
}
