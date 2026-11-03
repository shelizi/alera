import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path/path.dart' as p;

/// Relative roots use forward slashes; '' means the workspace root.
final workspaceGitRepositoriesProvider = FutureProvider.autoDispose
    .family<List<String>, String>((ref, workspacePath) {
      return const WorkspaceGitRepositoryScanner().scan(workspacePath);
    });

typedef _DirectoryToScan = ({
  Directory directory,
  String relativePath,
  int depth,
});

/// Detect independent repositories and submodules with direct .git markers.
/// Stay out of symlinks, worktree storage, and generated dependency directories.
final class WorkspaceGitRepositoryScanner {
  const WorkspaceGitRepositoryScanner({
    this.maxDepth = 8,
    this.maxDirectories = 2500,
    this.maxEntries = 20000,
    this.maxRepositories = 100,
  });

  final int maxDepth;
  final int maxDirectories;
  final int maxEntries;
  final int maxRepositories;

  static const Set<String> _excludedDirectories = <String>{
    '.git',
    '.worktrees',
    '.dart_tool',
    '.gradle',
    '.idea',
    '.next',
    '.nuxt',
    '.turbo',
    '.venv',
    '.yarn',
    'build',
    'dist',
    'node_modules',
    'target',
  };

  Future<List<String>> scan(String workspacePath) async {
    final root = Directory(workspacePath);
    if (!await root.exists()) return const <String>[];

    final pending = <_DirectoryToScan>[
      (directory: root, relativePath: '', depth: 0),
    ];
    final repositories = <String>[];
    var checkedDirectories = 0;
    var checkedEntries = 0;
    var nextDirectory = 0;

    while (nextDirectory < pending.length &&
        checkedDirectories < maxDirectories &&
        checkedEntries < maxEntries &&
        repositories.length < maxRepositories) {
      final current = pending[nextDirectory++];
      checkedDirectories++;
      // A linked worktree or submodule may have a .git file, not a directory.
      final marker = p.join(current.directory.path, '.git');
      try {
        if (await Directory(marker).exists() || await File(marker).exists()) {
          repositories.add(current.relativePath);
        }
      } on FileSystemException {
        // The directory may disappear during a scan.
      }

      if (current.depth >= maxDepth) continue;
      final children = <Directory>[];
      try {
        await for (final entry in current.directory.list(followLinks: false)) {
          checkedEntries++;
          if (checkedEntries >= maxEntries) break;
          if (entry is! Directory) continue;
          if (_excludedDirectories.contains(
            p.basename(entry.path).toLowerCase(),
          )) {
            continue;
          }
          children.add(entry);
        }
      } on FileSystemException {
        // One inaccessible directory must not hide sibling repositories.
      }
      children.sort(
        (a, b) => a.path.toLowerCase().compareTo(b.path.toLowerCase()),
      );
      for (final child in children) {
        final name = p.basename(child.path);
        pending.add((
          directory: child,
          relativePath: current.relativePath.isEmpty
              ? name
              : current.relativePath + '/' + name,
          depth: current.depth + 1,
        ));
      }
    }

    repositories.sort((a, b) {
      if (a.isEmpty) return -1;
      if (b.isEmpty) return 1;
      return a.toLowerCase().compareTo(b.toLowerCase());
    });
    return List<String>.unmodifiable(repositories);
  }
}
