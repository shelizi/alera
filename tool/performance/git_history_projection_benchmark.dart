// ignore_for_file: avoid_print

import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_history_graph.dart';

const int _sampleCount = 5;
int _sink = 0;

void main() {
  print('git history projection benchmark');
  print('samples=$_sampleCount, median microseconds per projection');
  print('scenario,size,median_us,min_us,max_us');

  for (final size in <int>[50, 500, 5000]) {
    _runScenario('linear', _linearHistory(size));
    _runScenario('merge-heavy', _mergeHeavyHistory(size));
  }

  if (_sink == -1) {
    print('unreachable');
  }
}

void _runScenario(String scenario, List<GitHistoryItem> items) {
  final iterations = switch (items.length) {
    <= 50 => 100,
    <= 500 => 20,
    _ => 2,
  };

  for (var warmup = 0; warmup < 2; warmup += 1) {
    _project(items, iterations);
  }

  final samples = <double>[
    for (var sample = 0; sample < _sampleCount; sample += 1)
      _project(items, iterations),
  ]..sort();
  final median = samples[samples.length ~/ 2];
  print(
    '$scenario,${items.length},${median.toStringAsFixed(1)},'
    '${samples.first.toStringAsFixed(1)},${samples.last.toStringAsFixed(1)}',
  );
}

double _project(List<GitHistoryItem> items, int iterations) {
  final stopwatch = Stopwatch()..start();
  for (var iteration = 0; iteration < iterations; iteration += 1) {
    final viewModels = buildGitHistoryViewModelsFromItems(items);
    _sink += viewModels.length;
    if (viewModels.isNotEmpty) {
      _sink += viewModels.last.outputSwimlanes.length;
    }
  }
  stopwatch.stop();
  return stopwatch.elapsedMicroseconds / iterations;
}

List<GitHistoryItem> _linearHistory(int size) {
  return <GitHistoryItem>[
    for (var index = 0; index < size; index += 1)
      GitHistoryItem(
        id: 'c$index',
        parentIds: index + 1 < size ? <String>['c${index + 1}'] : const [],
        subject: 'Commit $index',
        message: 'Commit $index',
      ),
  ];
}

List<GitHistoryItem> _mergeHeavyHistory(int size) {
  return <GitHistoryItem>[
    for (var index = 0; index < size; index += 1)
      GitHistoryItem(
        id: 'c$index',
        parentIds: <String>[
          if (index + 1 < size) 'c${index + 1}',
          if (index % 8 == 0 && index + 16 < size) 'c${index + 16}',
        ],
        subject: 'Commit $index',
        message: 'Commit $index',
      ),
  ];
}
