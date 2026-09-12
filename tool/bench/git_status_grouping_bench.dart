// Benchmark: GitChangeGroup.fromEntries and unifiedFromEntries performance.
// Run: dart run tool/bench/git_status_grouping_bench.dart
// No external deps - plain dart:math only.

import 'dart:math';

import 'package:alera/src/shared/infra/git/git_diff_models.dart';

// ---------------------------------------------------------------------------
// Path / name pools
// ---------------------------------------------------------------------------

const _features = <String>[
  'auth', 'billing', 'chat', 'codegen', 'dashboard', 'diff_viewer',
  'editor', 'file_tree', 'git_panel', 'history', 'keyboard', 'landing',
  'notifications', 'onboarding', 'preview', 'process_monitor', 'profile',
  'release', 'remote_sync', 'resource_manager', 'review', 'search',
  'settings', 'shell', 'sidebar', 'snippet', 'split_view', 'status_bar',
  'terminal', 'theme', 'timeline', 'toolbar', 'treesitter', 'updater',
  'vcs', 'welcome', 'workbench', 'workspace', 'xterm', 'yaml_editor',
];

const _subDirs = <String>[
  'presentation', 'application', 'domain', 'infra',
];

const _leafNames = <String>[
  'controller', 'provider', 'repository', 'service', 'model', 'state',
  'widget', 'screen', 'dialog', 'card', 'tile', 'button', 'icon',
  'formatter', 'parser', 'validator', 'mapper', 'adapter', 'handler',
  'listener', 'notifier', 'event', 'action', 'selector', 'builder',
  'resolver', 'scanner', 'loader', 'watcher', 'manager', 'runner',
  'codec', 'serializer', 'config', 'constants', 'extensions', 'utils_x',
  'types', 'exceptions', 'bridge', 'registry', 'factory', 'store',
  'cache', 'queue', 'stream_mixin', 'lifecycle', 'coordinator', 'schema',
  'snapshot', 'cursor', 'index', 'metadata', 'token', 'payload',
  'transformer', 'observer', 'dispatcher', 'emitter', 'interceptor',
];

const _extensions = <String>['.dart', '.dart', '.dart', '.rs', '.md', '.yaml', '.toml'];

const _topDirs = <String>[
  'lib/src/features',
  'lib/src/design_system',
  'lib/src/shared',
  'test/unit',
  'test/integration',
  'rust/src',
  'tool',
  'assets',
];

const _submoduleNames = <String>[
  'engine', 'proto', 'sdk', 'vendor', 'platform_libs',
  'runtime', 'codex_core', 'design_tokens', 'bridge_gen', 'ggml',
];

// ---------------------------------------------------------------------------
// Entry generator
// ---------------------------------------------------------------------------

List<GitChangeEntry> generateEntries(int count, {int seed = 42}) {
  final rng = Random(seed);
  final entries = <GitChangeEntry>[];

  for (var i = 0; i < count; i++) {
    final area = _pickArea(rng);
    final status = _pickStatus(rng, area);
    final path = _buildPath(rng, i);
    final isRename = status == GitChangeStatus.renamed;
    final oldPath = isRename ? _buildPath(rng, i + 100000) : null;
    final isBinary = rng.nextDouble() < 0.02;
    final isLarge = !isBinary && rng.nextDouble() < 0.01;
    final added = isBinary || isLarge ? null : rng.nextInt(200);
    final removed = isBinary || isLarge ? null : rng.nextInt(80);

    // ~0.5% submodule entries.
    GitSubmoduleStatus? submodule;
    String? submoduleRoot;
    if (area != GitChangeArea.untracked && rng.nextDouble() < 0.005) {
      final ns = rng.nextBool() ? 'third_party' : 'packages';
      final name = _submoduleNames[rng.nextInt(_submoduleNames.length)];
      submoduleRoot = '$ns/$name';
      submodule = GitSubmoduleStatus(
        commitChanged: rng.nextBool(),
        trackedChanges: rng.nextBool(),
        untrackedChanges: rng.nextBool(),
        inspectable: rng.nextBool(),
      );
    }

    entries.add(GitChangeEntry(
      path: path,
      area: area,
      status: status,
      oldPath: oldPath,
      added: added,
      removed: removed,
      isBinary: isBinary,
      isLarge: isLarge,
      submodule: submodule,
      submoduleRoot: submoduleRoot,
    ));
  }

  return entries;
}

GitChangeArea _pickArea(Random rng) {
  final v = rng.nextDouble();
  if (v < 0.15) return GitChangeArea.staged;
  if (v < 0.70) return GitChangeArea.unstaged;
  return GitChangeArea.untracked;
}

GitChangeStatus _pickStatus(Random rng, GitChangeArea area) {
  if (area == GitChangeArea.untracked) return GitChangeStatus.untracked;
  if (area == GitChangeArea.staged) {
    final v = rng.nextDouble();
    if (v < 0.50) return GitChangeStatus.modified;
    if (v < 0.70) return GitChangeStatus.added;
    if (v < 0.85) return GitChangeStatus.deleted;
    return GitChangeStatus.renamed;
  }
  // unstaged
  final v = rng.nextDouble();
  if (v < 0.72) return GitChangeStatus.modified;
  if (v < 0.88) return GitChangeStatus.deleted;
  return GitChangeStatus.renamed;
}

String _buildPath(Random rng, int salt) {
  final top = _topDirs[rng.nextInt(_topDirs.length)];
  final depth = 1 + rng.nextInt(6); // 1-6 extra segments

  final buf = StringBuffer(top);

  if (top == 'lib/src/features') {
    buf.write('/');
    buf.write(_features[rng.nextInt(_features.length)]);
    if (depth >= 2) {
      buf.write('/');
      buf.write(_subDirs[rng.nextInt(_subDirs.length)]);
    }
    for (var d = 2; d < depth; d++) {
      buf.write('/');
      buf.write(_leafNames[rng.nextInt(_leafNames.length)]);
    }
  } else {
    for (var d = 0; d < depth; d++) {
      buf.write('/');
      buf.write(_leafNames[(rng.nextInt(_leafNames.length) + salt) % _leafNames.length]);
    }
  }

  final ext = _extensions[rng.nextInt(_extensions.length)];
  buf.write(ext);
  return buf.toString();
}

// ---------------------------------------------------------------------------
// Benchmark harness
// ---------------------------------------------------------------------------

const _sizes = <int>[1000, 5000, 20000, 50000];
const _iterations = 5;
const _frameBudgetMs = 16.6;

void main() {
  print('Git status grouping benchmark\n');

  final fromResults = <int, List<double>>{};
  final unifiedResults = <int, List<double>>{};

  for (final size in _sizes) {
    final entries = generateEntries(size);

    // Warmup - excluded from timing.
    final warmupGroups = GitChangeGroup.fromEntries(entries);
    final warmupUnified = GitChangeGroup.unifiedFromEntries(entries);
    // Use values so the VM keeps the calls.
    var sink = warmupGroups.length + warmupUnified.length;
    for (final g in warmupGroups) { sink += g.treeRows.length; }
    for (final g in warmupUnified) { sink += g.treeRows.length; }

    final fromTimes = <double>[];
    var fromSink = 0;
    for (var iter = 0; iter < _iterations; iter++) {
      final sw = Stopwatch()..start();
      final groups = GitChangeGroup.fromEntries(entries);
      sw.stop();
      fromTimes.add(sw.elapsedMicroseconds / 1000.0);
      fromSink += groups.length;
      for (final g in groups) { fromSink += g.treeRows.length; }
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
      for (final g in groups) { unifiedSink += g.treeRows.length; }
    }
    unifiedResults[size] = unifiedTimes;

    // Prevent dead-code elimination.
    if (sink + fromSink + unifiedSink < 0) print('(never)');
  }

  _printTable('GitChangeGroup.fromEntries', fromResults);
  _printTable('GitChangeGroup.unifiedFromEntries', unifiedResults);
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