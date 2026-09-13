// ignore_for_file: avoid_print

part of 'git_status_real_repo_bench.dart';

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

  final areaGroups = <List<GitChangeEntry>>[
    staged,
    unstaged,
    untracked,
  ].where((group) => group.isNotEmpty).toList();

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
      result.add(
        GitChangeEntry(
          path: path,
          area: src.area,
          status: src.status,
          oldPath: oldPath,
        ),
      );
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
    final label = size == realSize
        ? '$size entries (real)'
        : '$size entries (scaled)';
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
