import 'dart:io';

import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
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

/// A lightweight, read-only, repository-scoped snapshot. Paths in two Git
/// repositories must never be combined into the same status or branch.
class const GitRepositoryOverviewSummary({
  required this.branch,
  required this.changedFiles,
  required this.ahead,
  required this.behind,
  required this.conflicts,
}) {
  final String? branch;
  final int changedFiles;
  final int ahead;
  final int behind;
  final bool conflicts;

  bool get available => branch != null;

  const new unavailable()
    : this(
        branch: null,
        changedFiles: 0,
        ahead: 0,
        behind: 0,
        conflicts: false,
      );
}

/// Loads at most [batchSize] repositories simultaneously, emitting partial
/// results to keep large multi-repo workspaces responsive. Individual failures
/// remain visible without failing the remaining repositories.
Stream<Map<String, GitRepositoryOverviewSummary>> loadGitRepositoryOverview({
  required String workspacePath,
  required Future<List<String>> roots,
  required Future<GitStatusResult> Function(String path) loadStatus,
  required Future<GitRepositoryState> Function(String path) loadRepositoryState,
  int batchSize = 3,
  bool Function()? isCancelled,
}) async* {
  if (batchSize < 1) throw ArgumentError.value(batchSize, 'batchSize');
  final discovered = await roots;
  final summaries = <String, GitRepositoryOverviewSummary>{};
  if (discovered.isEmpty) {
    yield const <String, GitRepositoryOverviewSummary>{};
  }
  for (var start = 0; start < discovered.length; start += batchSize) {
    if (isCancelled?.call() ?? false) return;
    final batch = discovered.skip(start).take(batchSize);
    final results = await Future.wait(
      batch.map((relativeRoot) async {
        final path = relativeRoot.isEmpty
            ? workspacePath
            : p.joinAll(<String>[workspacePath, ...relativeRoot.split('/')]);
        try {
          final values = await Future.wait<Object>(<Future<Object>>[
            loadStatus(path),
            loadRepositoryState(path),
          ]);
          final status = values[0] as GitStatusResult;
          final repo = values[1] as GitRepositoryState;
          return MapEntry(
            relativeRoot,
            GitRepositoryOverviewSummary(
              branch: repo.branch,
              changedFiles: status.entries
                  .map((entry) => entry.path)
                  .toSet()
                  .length,
              ahead: repo.ahead,
              behind: repo.behind,
              conflicts: repo.hasConflicts,
            ),
          );
        } on Object {
          return MapEntry(
            relativeRoot,
            const GitRepositoryOverviewSummary.unavailable(),
          );
        }
      }),
    );
    if (isCancelled?.call() ?? false) return;
    for (final entry in results) {
      summaries[entry.key] = entry.value;
    }
    yield Map<String, GitRepositoryOverviewSummary>.unmodifiable(summaries);
  }
}

/// Lazy: the sidebar watches this provider only when the overview expands.
/// Closing the overview stops scheduling more Git reads after the current batch.
final workspaceGitRepositoryOverviewProvider = StreamProvider.autoDispose
    .family<Map<String, GitRepositoryOverviewSummary>, String>((
      ref,
      workspacePath,
    ) {
      var disposed = false;
      ref.onDispose(() => disposed = true);
      final roots = ref.watch(
        workspaceGitRepositoriesProvider(workspacePath).future,
      );
      final backend = ref.read(gitBackendProvider);
      return loadGitRepositoryOverview(
        workspacePath: workspacePath,
        roots: roots,
        loadStatus: backend.status,
        loadRepositoryState: backend.repositoryState,
        isCancelled: () => disposed,
      );
    });
