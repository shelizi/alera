part of 'git_diff_models.dart';

class const GitChangeGroup({
  required final GitChangeArea area,
  required final List<GitChangeEntry> entries,
  required final List<GitChangeTreeRow> treeRows,
  this.entryIndices,
  this.unified = false,
}) {
  /// Native status projections keep indices into `GitStatusResult.entries` so
  /// refresh rebinding can skip an object-identity HashMap. Dart-built and
  /// legacy/mock groups leave this null and use the compatibility fallback.
  final List<int>? entryIndices;

  /// When true, the group holds files from every area in one list. [area] is
  /// only used as a collapse-key / bulk-action sentinel for the section.
  final bool unified;

  String get label => unified ? 'Changes' : area.label;

  static List<GitChangeGroup> fromEntries(List<GitChangeEntry> entries) {
    if (entries.isEmpty) {
      return const <GitChangeGroup>[];
    }
    final staged = <GitChangeEntry>[];
    final unstaged = <GitChangeEntry>[];
    final untracked = <GitChangeEntry>[];

    for (var i = 0; i < entries.length; i++) {
      final entry = entries[i];
      switch (entry.area) {
        case GitChangeArea.staged:
          staged.add(entry);
        case GitChangeArea.unstaged:
          unstaged.add(entry);
        case GitChangeArea.untracked:
          untracked.add(entry);
      }
    }

    return <GitChangeGroup>[
      if (staged.isNotEmpty) _groupFor(.staged, staged),
      if (unstaged.isNotEmpty) _groupFor(.unstaged, unstaged),
      if (untracked.isNotEmpty) _groupFor(.untracked, untracked),
    ];
  }

  /// Single Changes section with every entry sorted by path, then area so a
  /// staged and unstaged copy of the same file stay adjacent (staged first).
  static List<GitChangeGroup> unifiedFromEntries(List<GitChangeEntry> entries) {
    if (entries.isEmpty) {
      return const <GitChangeGroup>[];
    }
    final sorted = List<GitChangeEntry>.of(entries)
      ..sort(_compareUnifiedEntries);
    return <GitChangeGroup>[
      GitChangeGroup(
        area: .unstaged,
        entries: sorted,
        treeRows: _treeRows(sorted),
        unified: true,
      ),
    ];
  }

  static Future<List<GitChangeGroup>> fromEntriesChunked(
    List<GitChangeEntry> entries, {
    int chunkSize = gitStatusWorkChunkSize,
    void Function(double milliseconds)? onChunk,
  }) async {
    _validateGitStatusChunkSize(chunkSize);
    if (entries.isEmpty) {
      return const <GitChangeGroup>[];
    }

    final chunker = _GitStatusChunker(onChunk, chunkSize);
    try {
      final staged = <GitChangeEntry>[];
      final unstaged = <GitChangeEntry>[];
      final untracked = <GitChangeEntry>[];
      for (var index = 0; index < entries.length; index += 1) {
        final entry = entries[index];
        switch (entry.area) {
          case GitChangeArea.staged:
            staged.add(entry);
          case GitChangeArea.unstaged:
            unstaged.add(entry);
          case GitChangeArea.untracked:
            untracked.add(entry);
        }
        if ((index + 1) % chunkSize == 0) {
          await chunker.pause();
        }
      }

      final groups = <GitChangeGroup>[];
      for (final (area, areaEntries) in <(GitChangeArea, List<GitChangeEntry>)>[
        (.staged, staged),
        (.unstaged, unstaged),
        (.untracked, untracked),
      ]) {
        if (areaEntries.isEmpty) {
          continue;
        }
        await _sortListChunked(
          areaEntries,
          _compareEntryPath,
          chunkSize,
          chunker,
        );
        final treeRows = await _treeRowsChunked(
          areaEntries,
          chunkSize,
          chunker,
        );
        groups.add(
          GitChangeGroup(area: area, entries: areaEntries, treeRows: treeRows),
        );
        await chunker.pause();
      }
      return groups;
    } finally {
      chunker.finish();
    }
  }

  static Future<List<GitChangeGroup>> unifiedFromEntriesChunked(
    List<GitChangeEntry> entries, {
    int chunkSize = gitStatusWorkChunkSize,
    void Function(double milliseconds)? onChunk,
  }) async {
    _validateGitStatusChunkSize(chunkSize);
    if (entries.isEmpty) {
      return const <GitChangeGroup>[];
    }

    final chunker = _GitStatusChunker(onChunk, chunkSize);
    try {
      final sorted = <GitChangeEntry>[];
      for (var index = 0; index < entries.length; index += 1) {
        sorted.add(entries[index]);
        if ((index + 1) % chunkSize == 0) {
          await chunker.pause();
        }
      }
      await _sortListChunked(
        sorted,
        _compareUnifiedEntries,
        chunkSize,
        chunker,
      );
      final treeRows = await _treeRowsChunked(sorted, chunkSize, chunker);
      return <GitChangeGroup>[
        GitChangeGroup(
          area: .unstaged,
          entries: sorted,
          treeRows: treeRows,
          unified: true,
        ),
      ];
    } finally {
      chunker.finish();
    }
  }

  static Future<List<GitChangeGroup>> rebindEntryInstancesChunked(
    List<GitChangeGroup> groups, {
    required List<GitChangeEntry> sourceEntries,
    required List<GitChangeEntry> reboundEntries,
    int chunkSize = gitStatusWorkChunkSize,
    void Function(double milliseconds)? onChunk,
  }) async {
    _validateGitStatusChunkSize(chunkSize);
    if (identical(sourceEntries, reboundEntries)) {
      return groups;
    }
    if (groups.isEmpty) {
      return fromEntriesChunked(
        reboundEntries,
        chunkSize: chunkSize,
        onChunk: onChunk,
      );
    }

    final chunker = _GitStatusChunker(onChunk, chunkSize);
    try {
      final canRebindByIndex =
          sourceEntries.length == reboundEntries.length &&
          groups.every((group) {
            final indices = group.entryIndices;
            if (indices == null || indices.length != group.entries.length) {
              return false;
            }
            return group.treeRows.every(
              (row) => row.entry == null || row.entryIndex != null,
            );
          });
      if (canRebindByIndex) {
        final reboundGroups = <GitChangeGroup>[];
        for (final group in groups) {
          final indices = group.entryIndices!;
          final groupEntries = <GitChangeEntry>[];
          for (var index = 0; index < indices.length; index += 1) {
            final entryIndex = indices[index];
            if (entryIndex < 0 || entryIndex >= reboundEntries.length) {
              throw StateError(
                'Git status group index $entryIndex is outside the '
                '${reboundEntries.length}-entry status result.',
              );
            }
            groupEntries.add(reboundEntries[entryIndex]);
            if ((index + 1) % chunkSize == 0) {
              await chunker.pause();
            }
          }

          final treeRows = <GitChangeTreeRow>[];
          for (var index = 0; index < group.treeRows.length; index += 1) {
            final row = group.treeRows[index];
            final entryIndex = row.entryIndex;
            GitChangeEntry? entry;
            if (entryIndex != null) {
              if (entryIndex < 0 || entryIndex >= reboundEntries.length) {
                throw StateError(
                  'Git status tree index $entryIndex is outside the '
                  '${reboundEntries.length}-entry status result.',
                );
              }
              entry = reboundEntries[entryIndex];
            }
            treeRows.add(
              GitChangeTreeRow(
                kind: row.kind,
                name: row.name,
                path: row.path,
                depth: row.depth,
                fileCount: row.fileCount,
                entry: entry,
                entryIndex: entryIndex,
              ),
            );
            if ((index + 1) % chunkSize == 0) {
              await chunker.pause();
            }
          }

          reboundGroups.add(
            GitChangeGroup(
              area: group.area,
              entries: List<GitChangeEntry>.unmodifiableOf(groupEntries),
              treeRows: List<GitChangeTreeRow>.unmodifiableOf(treeRows),
              entryIndices: indices,
              unified: group.unified,
            ),
          );
          await chunker.pause();
        }
        return List<GitChangeGroup>.unmodifiableOf(reboundGroups);
      }

      final reboundBySource = <GitChangeEntry, GitChangeEntry>{};
      final entryCount = sourceEntries.length < reboundEntries.length
          ? sourceEntries.length
          : reboundEntries.length;
      for (var index = 0; index < entryCount; index += 1) {
        reboundBySource[sourceEntries[index]] = reboundEntries[index];
        if ((index + 1) % chunkSize == 0) {
          await chunker.pause();
        }
      }

      final reboundGroups = <GitChangeGroup>[];
      for (final group in groups) {
        final groupEntries = <GitChangeEntry>[];
        for (var index = 0; index < group.entries.length; index += 1) {
          final entry = group.entries[index];
          groupEntries.add(reboundBySource[entry] ?? entry);
          if ((index + 1) % chunkSize == 0) {
            await chunker.pause();
          }
        }

        final treeRows = <GitChangeTreeRow>[];
        for (var index = 0; index < group.treeRows.length; index += 1) {
          final row = group.treeRows[index];
          final entry = row.entry;
          treeRows.add(
            GitChangeTreeRow(
              kind: row.kind,
              name: row.name,
              path: row.path,
              depth: row.depth,
              fileCount: row.fileCount,
              entry: entry == null ? null : reboundBySource[entry] ?? entry,
              entryIndex: row.entryIndex,
            ),
          );
          if ((index + 1) % chunkSize == 0) {
            await chunker.pause();
          }
        }

        reboundGroups.add(
          GitChangeGroup(
            area: group.area,
            entries: groupEntries,
            treeRows: treeRows,
            entryIndices: group.entryIndices,
            unified: group.unified,
          ),
        );
        await chunker.pause();
      }
      return reboundGroups;
    } finally {
      chunker.finish();
    }
  }

  static GitChangeGroup _groupFor(
    GitChangeArea area,
    List<GitChangeEntry> entries,
  ) {
    entries.sort(_compareEntryPath);
    return GitChangeGroup(
      area: area,
      entries: entries,
      treeRows: _treeRows(entries),
    );
  }

  static int _compareEntryPath(GitChangeEntry a, GitChangeEntry b) =>
      a.path.compareTo(b.path);

  static int _compareUnifiedEntries(GitChangeEntry a, GitChangeEntry b) {
    final byPath = a.path.compareTo(b.path);
    if (byPath != 0) {
      return byPath;
    }
    return _areaSortIndex(a.area).compareTo(_areaSortIndex(b.area));
  }

  static int _areaSortIndex(GitChangeArea area) {
    return switch (area) {
      GitChangeArea.staged => 0,
      GitChangeArea.unstaged => 1,
      GitChangeArea.untracked => 2,
    };
  }

  static List<GitChangeTreeRow> _treeRows(List<GitChangeEntry> entries) {
    if (entries.isEmpty) {
      return const <GitChangeTreeRow>[];
    }

    final root = _GitChangeTreeNode(name: '', path: '', depth: 0);
    final dirMap = <String, _GitChangeTreeNode>{'': root};

    _GitChangeTreeNode? lastParent;
    String? lastDirPath;

    for (var i = 0; i < entries.length; i++) {
      final entry = entries[i];
      final path = entry.path;
      final lastSlash = path.lastIndexOf('/');

      if (lastSlash == -1) {
        if (path.isEmpty) {
          continue;
        }
        root.addFileRow(
          GitChangeTreeRow(
            kind: GitChangeTreeRowKind.file,
            name: path,
            path: path,
            depth: 0,
            fileCount: 1,
            entry: entry,
          ),
        );
        continue;
      }

      final fileName = path.substring(lastSlash + 1);
      if (fileName.isEmpty) {
        continue;
      }

      // Path-sorted input usually repeats the same parent; skip the map.
      final _GitChangeTreeNode parent;
      if (lastDirPath != null &&
          lastDirPath.length == lastSlash &&
          path.startsWith(lastDirPath)) {
        parent = lastParent!;
      } else {
        final dirPath = path.substring(0, lastSlash);
        parent = dirMap[dirPath] ?? _ensureDir(dirPath, dirMap, root);
        lastDirPath = dirPath;
        lastParent = parent;
      }

      parent.addFileRow(
        GitChangeTreeRow(
          kind: GitChangeTreeRowKind.file,
          name: fileName,
          path: path,
          depth: parent.depth + 1,
          fileCount: 1,
          entry: entry,
        ),
      );
    }

    final rootSubs = root.subdirectories;
    if (rootSubs != null) {
      for (var i = 0; i < rootSubs.length; i++) {
        rootSubs[i].finalizeTree();
      }
      if (rootSubs.length > 1) {
        rootSubs.sort((a, b) => a.name.compareTo(b.name));
      }
    }

    final rows = <GitChangeTreeRow>[];
    if (rootSubs != null) {
      for (var i = 0; i < rootSubs.length; i++) {
        rootSubs[i].appendRows(rows);
      }
    }
    final rootFiles = root.fileRows;
    if (rootFiles != null) {
      for (var i = 0; i < rootFiles.length; i++) {
        rows.add(rootFiles[i]);
      }
    }
    return rows;
  }

  static Future<List<GitChangeTreeRow>> _treeRowsChunked(
    List<GitChangeEntry> entries,
    int chunkSize,
    _GitStatusChunker chunker,
  ) async {
    if (entries.isEmpty) {
      return const <GitChangeTreeRow>[];
    }

    final root = _GitChangeTreeNode(name: '', path: '', depth: 0);
    final dirMap = <String, _GitChangeTreeNode>{'': root};

    _GitChangeTreeNode? lastParent;
    String? lastDirPath;

    for (var index = 0; index < entries.length; index += 1) {
      final entry = entries[index];
      final path = entry.path;
      final lastSlash = path.lastIndexOf('/');

      if (lastSlash == -1) {
        if (path.isNotEmpty) {
          root.addFileRow(
            GitChangeTreeRow(
              kind: GitChangeTreeRowKind.file,
              name: path,
              path: path,
              depth: 0,
              fileCount: 1,
              entry: entry,
            ),
          );
        }
      } else {
        final fileName = path.substring(lastSlash + 1);
        if (fileName.isNotEmpty) {
          // Path-sorted input usually repeats the same parent; skip the map.
          final _GitChangeTreeNode parent;
          if (lastDirPath != null &&
              lastDirPath.length == lastSlash &&
              path.startsWith(lastDirPath)) {
            parent = lastParent!;
          } else {
            final dirPath = path.substring(0, lastSlash);
            parent = dirMap[dirPath] ?? _ensureDir(dirPath, dirMap, root);
            lastDirPath = dirPath;
            lastParent = parent;
          }

          parent.addFileRow(
            GitChangeTreeRow(
              kind: GitChangeTreeRowKind.file,
              name: fileName,
              path: path,
              depth: parent.depth + 1,
              fileCount: 1,
              entry: entry,
            ),
          );
        }
      }

      await chunker.checkpoint();
    }

    final rootSubs = root.subdirectories;
    if (rootSubs != null) {
      for (var index = 0; index < rootSubs.length; index += 1) {
        await rootSubs[index].finalizeTreeChunked(chunkSize, chunker);
        await chunker.checkpoint();
      }
      if (rootSubs.length > 1) {
        await _sortListChunked(
          rootSubs,
          (a, b) => a.name.compareTo(b.name),
          chunkSize,
          chunker,
        );
      }
    }

    final rows = <GitChangeTreeRow>[];
    if (rootSubs != null) {
      for (final rootSub in rootSubs) {
        await rootSub.appendRowsChunked(rows, chunkSize, chunker);
      }
    }
    final rootFiles = root.fileRows;
    if (rootFiles != null) {
      for (var index = 0; index < rootFiles.length; index += 1) {
        rows.add(rootFiles[index]);
        await chunker.checkpoint();
      }
    }
    return rows;
  }

  static _GitChangeTreeNode _ensureDir(
    String dirPath,
    Map<String, _GitChangeTreeNode> dirMap,
    _GitChangeTreeNode root,
  ) {
    final lastSlash = dirPath.lastIndexOf('/');
    final _GitChangeTreeNode parent;
    final String name;
    final int depth;

    if (lastSlash == -1) {
      parent = root;
      name = dirPath;
      depth = 0;
    } else {
      final parentPath = dirPath.substring(0, lastSlash);
      parent = dirMap[parentPath] ?? _ensureDir(parentPath, dirMap, root);
      name = dirPath.substring(lastSlash + 1);
      depth = parent.depth + 1;
    }

    final node = _GitChangeTreeNode(name: name, path: dirPath, depth: depth);
    dirMap[dirPath] = node;
    parent.addSubdirectory(node);
    return node;
  }
}

void _validateGitStatusChunkSize(int chunkSize) {
  if (chunkSize < 1) {
    throw ArgumentError.value(chunkSize, 'chunkSize', 'must be positive');
  }
}
