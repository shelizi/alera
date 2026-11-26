// ignore_for_file: avoid_print, implementation_imports

/// Exercises the real generated Dart -> Rust bridge and backend, not a fake.
/// Pass the freshly built native library path.
library;

import 'dart:io';

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
    require((await backend.diffChangedPaths(root.path))!.isEmpty, 'clean repo');
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
    final combined = await backend.diffAll(path: root.path);
    require(
      combined.files.any((file) => file.path == 'modules/sample/a.txt'),
      'combined renderer preserves nested change',
    );
    print(
      'Real RustGitBackend/FRB probe passed: empty, ordered list, fresh changes, page and null submodule fallback.',
    );
  } finally {
    temp.deleteSync(recursive: true);
  }
}
