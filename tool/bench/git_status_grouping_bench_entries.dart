part of 'git_status_grouping_bench.dart';

// ---------------------------------------------------------------------------
// Path / name pools
// ---------------------------------------------------------------------------

const _features = <String>[
  'auth',
  'billing',
  'chat',
  'codegen',
  'dashboard',
  'diff_viewer',
  'editor',
  'file_tree',
  'git_panel',
  'history',
  'keyboard',
  'landing',
  'notifications',
  'onboarding',
  'preview',
  'process_monitor',
  'profile',
  'release',
  'remote_sync',
  'resource_manager',
  'review',
  'search',
  'settings',
  'shell',
  'sidebar',
  'snippet',
  'split_view',
  'status_bar',
  'terminal',
  'theme',
  'timeline',
  'toolbar',
  'treesitter',
  'updater',
  'vcs',
  'welcome',
  'workbench',
  'workspace',
  'xterm',
  'yaml_editor',
];

const _subDirs = <String>['presentation', 'application', 'domain', 'infra'];

const _leafNames = <String>[
  'controller',
  'provider',
  'repository',
  'service',
  'model',
  'state',
  'widget',
  'screen',
  'dialog',
  'card',
  'tile',
  'button',
  'icon',
  'formatter',
  'parser',
  'validator',
  'mapper',
  'adapter',
  'handler',
  'listener',
  'notifier',
  'event',
  'action',
  'selector',
  'builder',
  'resolver',
  'scanner',
  'loader',
  'watcher',
  'manager',
  'runner',
  'codec',
  'serializer',
  'config',
  'constants',
  'extensions',
  'utils_x',
  'types',
  'exceptions',
  'bridge',
  'registry',
  'factory',
  'store',
  'cache',
  'queue',
  'stream_mixin',
  'lifecycle',
  'coordinator',
  'schema',
  'snapshot',
  'cursor',
  'index',
  'metadata',
  'token',
  'payload',
  'transformer',
  'observer',
  'dispatcher',
  'emitter',
  'interceptor',
];

const _extensions = <String>[
  '.dart',
  '.dart',
  '.dart',
  '.rs',
  '.md',
  '.yaml',
  '.toml',
];

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
  'engine',
  'proto',
  'sdk',
  'vendor',
  'platform_libs',
  'runtime',
  'codex_core',
  'design_tokens',
  'bridge_gen',
  'ggml',
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

    entries.add(
      GitChangeEntry(
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
      ),
    );
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
      buf.write(
        _leafNames[(rng.nextInt(_leafNames.length) + salt) % _leafNames.length],
      );
    }
  }

  final ext = _extensions[rng.nextInt(_extensions.length)];
  buf.write(ext);
  return buf.toString();
}

List<GitChangeEntry> generateDeepUntracked(int count, {int seed = 7}) {
  final rng = Random(seed);
  const roots = <String>[
    '.tmp-alera-devin-live-e2e/target/debug',
    'alera-ewdk-verify/payload/data/flutter_assets',
    'alera-ewdk-verify/out/obj',
    'alera-ewdk-verify/src/flutter_wrapper',
  ];
  const mid = <String>[
    'fingerprint',
    'incremental',
    'deps',
    'icons',
    'assets',
    'packages',
  ];
  final entries = <GitChangeEntry>[];
  for (var i = 0; i < count; i++) {
    final extra = 3 + rng.nextInt(5);
    final buf = StringBuffer(roots[i % roots.length]);
    for (var depth = 0; depth < extra; depth++) {
      buf.write('/');
      buf.write(mid[(i + depth) % mid.length]);
      buf.write('-');
      buf.write(rng.nextInt(16).toRadixString(16));
    }
    buf.write('/file_$i.o');
    final untracked = i != 0;
    entries.add(
      GitChangeEntry(
        path: buf.toString(),
        area: untracked ? GitChangeArea.untracked : GitChangeArea.unstaged,
        status: untracked
            ? GitChangeStatus.untracked
            : GitChangeStatus.modified,
      ),
    );
  }
  return entries;
}

List<GitChangeEntry> loadRepoStatusEntries(String repoPath) {
  final result = Process.runSync(
    'git',
    <String>['-C', repoPath, 'status', '--porcelain=v1', '-uall'],
    stdoutEncoding: utf8,
    stderrEncoding: utf8,
  );
  if (result.exitCode != 0) {
    stderr.writeln('git status failed (${result.exitCode}): ${result.stderr}');
    exit(1);
  }
  return parsePorcelain(result.stdout.toString());
}

List<GitChangeEntry> parsePorcelain(String stdout) {
  final entries = <GitChangeEntry>[];
  for (final rawLine in stdout.split('\n')) {
    final line = rawLine.endsWith('\r')
        ? rawLine.substring(0, rawLine.length - 1)
        : rawLine;
    if (line.length < 4) {
      continue;
    }
    if (line.startsWith('?? ')) {
      entries.add(
        GitChangeEntry(
          path: line.substring(3),
          area: GitChangeArea.untracked,
          status: GitChangeStatus.untracked,
        ),
      );
      continue;
    }
    final stagedCode = line[0];
    final unstagedCode = line[1];
    var path = line.substring(3);
    String? oldPath;
    final arrow = path.indexOf(' -> ');
    if (arrow != -1 && (stagedCode == 'R' || unstagedCode == 'R')) {
      oldPath = path.substring(0, arrow);
      path = path.substring(arrow + 4);
    }
    if (stagedCode != ' ' && stagedCode != '?') {
      entries.add(
        GitChangeEntry(
          path: path,
          oldPath: oldPath,
          area: GitChangeArea.staged,
          status: _statusFromPorcelain(stagedCode),
        ),
      );
    }
    if (unstagedCode != ' ' && unstagedCode != '?') {
      entries.add(
        GitChangeEntry(
          path: path,
          oldPath: oldPath,
          area: GitChangeArea.unstaged,
          status: _statusFromPorcelain(unstagedCode),
        ),
      );
    }
  }
  return entries;
}

GitChangeStatus _statusFromPorcelain(String code) {
  return switch (code) {
    'A' => GitChangeStatus.added,
    'D' => GitChangeStatus.deleted,
    'R' => GitChangeStatus.renamed,
    'C' => GitChangeStatus.copied,
    'U' => GitChangeStatus.untracked,
    _ => GitChangeStatus.modified,
  };
}
