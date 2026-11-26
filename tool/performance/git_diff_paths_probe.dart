// ignore_for_file: avoid_print, implementation_imports

/// Exercises the real generated Dart -> Rust bridge and backend, not a fake.
/// Pass the freshly built native library path.
library;

import 'dart:io';

import 'package:alera/src/features/workbench/application/workspace_git_repository_change_count.dart';
import 'package:alera/src/rust/frb_generated.dart' as rust;
import 'package:alera/src/shared/infra/git/rust_git_backend.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

void require(bool value, String message) {
  if (!value) throw StateError(message);
}

// Git CLI is used only to construct isolated test repositories.
Future<void> fixtureGit(Directory root, List<String> args) async {
  final result = await Process.run('git', [
    '-c',
    'user.name=Test',
    '-c',
    'user.email=test@example.invalid',
    '-c',
    'core.autocrlf=false',
    ...args,
  ], workingDirectory: root.path);
  if (result.exitCode != 0) {
    throw StateError('Git fixture failed: ${result.stderr}');
  }
}

Future<void> main(List<String> args) async {
  if (args.length != 1) {
    throw ArgumentError('Supply the freshly built native library path');
  }
  await rust.RustLib.init(externalLibrary: ExternalLibrary.open(args.single));
  const backend = RustGitBackend();
  final temp = Directory.systemTemp.createTempSync('alera-git-diff-paths-');
  try {
    final root = Directory('${temp.path}/repo')..createSync();
    final child = Directory('${temp.path}/child')..createSync();
    for (final directory in [root, child]) {
      await fixtureGit(directory, ['init', '-b', 'main']);
      File('${directory.path}/a.txt').writeAsStringSync('old\n');
      await fixtureGit(directory, ['add', '.']);
      await fixtureGit(directory, ['commit', '-m', 'baseline']);
    }
    Future<int> overviewCount(String path) =>
        loadWorkspaceGitRepositoryChangeCount(
          path: path,
          loadChangedPaths: backend.diffChangedPaths,
          loadStatus: backend.status,
        );
    require((await backend.diffChangedPaths(root.path))!.isEmpty, 'clean repo');
    require(await overviewCount(root.path) == 0, 'clean overview count');
    File('${root.path}/a.txt').writeAsStringSync('staged\n');
    File('${root.path}/b.txt').writeAsStringSync('added\n');
    await fixtureGit(root, ['add', '.']);
    File('${root.path}/a.txt').writeAsStringSync('worktree\n');
    File('${root.path}/z.txt').writeAsStringSync('untracked');
    final paths = await backend.diffChangedPaths(root.path);
    final old = await backend.status(root.path);
    final expected = <String>{for (final entry in old.entries) entry.path};
    require(paths!.join('|') == expected.join('|'), 'full status order parity');
    require(paths.join('|') == 'z.txt|a.txt|b.txt', 'staged/unstaged dedup');
    require(
      await overviewCount(root.path) == expected.length,
      'overview count parity',
    );
    final page = await backend.diffAllPage(
      path: root.path,
      filePaths: [paths.first],
    );
    require(page.files.single.path == 'z.txt', 'native first page');
    File('${root.path}/z.txt').deleteSync();
    require(
      (await backend.diffChangedPaths(root.path))!.join('|') == 'a.txt|b.txt',
      'fresh path discovery',
    );
    await fixtureGit(root, ['add', '.']);
    await fixtureGit(root, ['commit', '-m', 'changes']);
    await fixtureGit(root, [
      '-c',
      'protocol.file.allow=always',
      'submodule',
      'add',
      child.path,
      'modules/sample',
    ]);
    await fixtureGit(root, ['commit', '-am', 'submodule']);
    File('${root.path}/modules/sample/a.txt')
        .writeAsStringSync('dirty child\n');
    require(
      await backend.diffChangedPaths(root.path) == null,
      'dirty submodule fallback',
    );
    require(
      await overviewCount(root.path) ==
          (await backend.status(root.path)).entries
              .map((entry) => entry.path)
              .toSet()
              .length,
      'dirty submodule overview fallback parity',
    );
    final combined = await backend.diffAll(path: root.path);
    require(
      combined.files.any((file) => file.path == 'modules/sample/a.txt'),
      'combined renderer preserves nested change',
    );
    print(
      'Real RustGitBackend/FRB probe passed: empty, ordered list, fresh changes, page, overview count and null submodule fallback.',
    );
    final benchmark = Directory('${temp.path}/overview-benchmark')
      ..createSync();
    await fixtureGit(benchmark, ['init', '-b', 'main']);
    File file(int i) =>
        File('${benchmark.path}/file-${i.toString().padLeft(4, '0')}.txt');
    for (var i = 0; i < 512; i++) {
      file(i).writeAsStringSync('baseline $i\n' * 80);
    }
    await fixtureGit(benchmark, ['add', '.']);
    await fixtureGit(benchmark, ['commit', '-m', 'baseline']);
    for (var i = 0; i < 160; i++) {
      file(i).writeAsStringSync('baseline $i\n' * 70 + 'staged\n');
    }
    await fixtureGit(benchmark, [
      'add',
      ...List.generate(80, (i) => file(i).uri.pathSegments.last),
    ]);
    for (var i = 0; i < 160; i++) {
      file(i).writeAsStringSync('baseline $i\n' * 70 + 'worktree\n');
    }
    var fullCalls = 0;
    var lightweightCalls = 0;
    Future<int> oldCount() async =>
        (await backend.status(benchmark.path)).entries
            .map((entry) => entry.path)
            .toSet()
            .length;
    Future<int> newCount() => loadWorkspaceGitRepositoryChangeCount(
      path: benchmark.path,
      loadChangedPaths: (path) {
        lightweightCalls++;
        return backend.diffChangedPaths(path);
      },
      loadStatus: (path) {
        fullCalls++;
        return backend.status(path);
      },
    );
    Future<double> measure(Future<int> Function() load) async {
      final watch = Stopwatch()..start();
      final count = await load();
      watch.stop();
      require(count == 160, 'benchmark count parity: $count');
      return watch.elapsedMicroseconds / 1000;
    }

    final full = await backend.status(benchmark.path);
    require(full.entries.length == 240, 'benchmark status fixture');
    await measure(oldCount);
    await measure(newCount);
    final before = <double>[];
    final after = <double>[];
    for (var i = 0; i < 7; i++) {
      if (i.isEven) {
        before.add(await measure(oldCount));
        after.add(await measure(newCount));
      } else {
        after.add(await measure(newCount));
        before.add(await measure(oldCount));
      }
    }
    before.sort();
    after.sort();
    require(
      lightweightCalls == 8 && fullCalls == 0,
      'overview avoids full status on normal repositories',
    );
    print(
      'Repository overview count (512 tracked, 160 changed, 240 status entries): old median ${before[3]} ms, new median ${after[3]} ms',
    );
    print('Old samples: $before; new samples: $after');
  } finally {
    temp.deleteSync(recursive: true);
  }
}
