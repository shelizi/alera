import 'dart:math' as math;

part 'git_range_models.dart';
part 'git_history_graph_models.dart';
part 'git_history_models.dart';

enum GitChangeArea(final String key) {
  untracked('untracked'),
  unstaged('unstaged'),
  staged('staged');

  String get label => switch (this) {
    GitChangeArea.untracked => 'Untracked',
    GitChangeArea.unstaged => 'Unstaged',
    GitChangeArea.staged => 'Staged',
  };
}

enum GitChangeStatus(final String badge) {
  modified('M'),
  added('A'),
  deleted('D'),
  renamed('R'),
  copied('C'),
  untracked('U'),
}

enum GitChangeTreeRowKind { directory, file }

enum GitDiffLineKind { addition, deletion, hunk, header, context }

/// Bounds the synchronous work between event-loop yields while projecting a
/// large status result.
const gitStatusWorkChunkSize = 16;

class _GitStatusChunker {
  _GitStatusChunker(this._onChunk, this._chunkSize) {
    if (_onChunk != null) {
      _stopwatch = Stopwatch()..start();
    }
  }

  final void Function(double milliseconds)? _onChunk;
  final int _chunkSize;
  int _operationCount = 0;
  Stopwatch? _stopwatch;

  Future<void> checkpoint() async {
    _operationCount += 1;
    if (_operationCount % _chunkSize == 0) {
      await pause();
    }
  }

  Future<void> pause() async {
    _record();
    await Future.pause();
    final stopwatch = _stopwatch;
    if (stopwatch != null) {
      stopwatch
        ..reset()
        ..start();
    }
  }

  void finish() {
    _record();
  }

  void _record() {
    final stopwatch = _stopwatch;
    final onChunk = _onChunk;
    if (stopwatch == null || onChunk == null) {
      return;
    }
    stopwatch.stop();
    onChunk(stopwatch.elapsedMicroseconds / 1000.0);
  }
}

class const GitStatusResult({
  required final List<GitChangeEntry> entries,
  final List<GitChangeGroup> groups = const [],
}) {
  List<GitChangeGroup> get effectiveGroups {
    if (groups.isNotEmpty) {
      return groups;
    }
    return GitChangeGroup.fromEntries(entries);
  }

  List<GitChangeEntry> entriesForPath(String relativePath) {
    return entries
        .where((entry) => entry.path == relativePath)
        .toList(growable: false);
  }
}

class const GitRepositoryState({
  required final String branch,
  final String? upstream,
  final int ahead = 0,
  final int behind = 0,
  final bool hasConflicts = false,
  final String? headMessage,
}) {
  bool get hasUpstream => upstream != null && upstream!.isNotEmpty;
  bool get hasHeadCommit => headMessage != null;
}

class const GitStashEntry({
  required final int index,
  required final String reference,
  required final String message,
  required final String oid,
});

class const GitChangeGroup({
  required final GitChangeArea area,
  required final List<GitChangeEntry> entries,
  required final List<GitChangeTreeRow> treeRows,
  this.unified = false,
}) {
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

Future<void> _sortListChunked<T>(
  List<T> values,
  int Function(T left, T right) compare,
  int chunkSize,
  _GitStatusChunker chunker,
) async {
  if (values.length < 2) {
    return;
  }

  final runSize = math.min(chunkSize, values.length);
  for (var start = 0; start < values.length; start += runSize) {
    final end = math.min(start + runSize, values.length);
    final run = values.sublist(start, end)..sort(compare);
    values.setRange(start, end, run);
    if (end < values.length) {
      await chunker.pause();
    }
  }
  if (runSize == values.length) {
    return;
  }

  var source = values;
  var target = List<T>.filled(values.length, values.first);
  for (var width = runSize; width < values.length; width *= 2) {
    for (var start = 0; start < values.length; start += width * 2) {
      final middle = math.min(start + width, values.length);
      final end = math.min(start + width * 2, values.length);
      var left = start;
      var right = middle;
      var destination = start;

      while (left < middle && right < end) {
        final leftValue = source[left];
        final rightValue = source[right];
        if (compare(leftValue, rightValue) <= 0) {
          target[destination] = leftValue;
          left += 1;
        } else {
          target[destination] = rightValue;
          right += 1;
        }
        destination += 1;
        await chunker.checkpoint();
      }
      while (left < middle) {
        target[destination] = source[left];
        left += 1;
        destination += 1;
        await chunker.checkpoint();
      }
      while (right < end) {
        target[destination] = source[right];
        right += 1;
        destination += 1;
        await chunker.checkpoint();
      }
    }

    final previousSource = source;
    source = target;
    target = previousSource;
    if (width > values.length ~/ 2) {
      break;
    }
  }

  if (!identical(source, values)) {
    for (var index = 0; index < values.length; index += 1) {
      values[index] = source[index];
      await chunker.checkpoint();
    }
  }
}

class _GitChangeTreeNode({
  required final String name,
  required final String path,
  required final int depth,
}) {
  List<_GitChangeTreeNode>? subdirectories;
  List<GitChangeTreeRow>? fileRows;
  int fileCount = 0;

  void addSubdirectory(_GitChangeTreeNode node) {
    (subdirectories ??= <_GitChangeTreeNode>[]).add(node);
  }

  void addFileRow(GitChangeTreeRow row) {
    (fileRows ??= <GitChangeTreeRow>[]).add(row);
  }

  void finalizeTree() {
    final subs = subdirectories;
    if (subs != null && subs.length > 1) {
      subs.sort((a, b) => a.name.compareTo(b.name));
    }
    var count = fileRows?.length ?? 0;
    if (subs != null) {
      for (var i = 0; i < subs.length; i++) {
        final sub = subs[i];
        sub.finalizeTree();
        count += sub.fileCount;
      }
    }
    fileCount = count;
  }

  Future<void> finalizeTreeChunked(
    int chunkSize,
    _GitStatusChunker chunker,
  ) async {
    final subs = subdirectories;
    if (subs != null && subs.length > 1) {
      await _sortListChunked(
        subs,
        (a, b) => a.name.compareTo(b.name),
        chunkSize,
        chunker,
      );
    }
    var count = fileRows?.length ?? 0;
    if (subs != null) {
      for (var index = 0; index < subs.length; index += 1) {
        final sub = subs[index];
        await sub.finalizeTreeChunked(chunkSize, chunker);
        count += sub.fileCount;
        await chunker.checkpoint();
      }
    }
    fileCount = count;
  }

  void appendRows(List<GitChangeTreeRow> rows) {
    rows.add(
      GitChangeTreeRow(
        kind: GitChangeTreeRowKind.directory,
        name: name,
        path: path,
        depth: depth,
        fileCount: fileCount,
        entry: null,
      ),
    );
    final subs = subdirectories;
    if (subs != null) {
      for (var i = 0; i < subs.length; i++) {
        subs[i].appendRows(rows);
      }
    }
    final files = fileRows;
    if (files != null) {
      for (var i = 0; i < files.length; i++) {
        rows.add(files[i]);
      }
    }
  }

  Future<void> appendRowsChunked(
    List<GitChangeTreeRow> rows,
    int chunkSize,
    _GitStatusChunker chunker,
  ) async {
    rows.add(
      GitChangeTreeRow(
        kind: GitChangeTreeRowKind.directory,
        name: name,
        path: path,
        depth: depth,
        fileCount: fileCount,
        entry: null,
      ),
    );
    await chunker.checkpoint();

    final subs = subdirectories;
    if (subs != null) {
      for (final sub in subs) {
        await sub.appendRowsChunked(rows, chunkSize, chunker);
      }
    }
    final files = fileRows;
    if (files != null) {
      for (final file in files) {
        rows.add(file);
        await chunker.checkpoint();
      }
    }
  }
}

class const GitChangeEntry({
  required final String path,
  required final GitChangeArea area,
  required final GitChangeStatus status,
  final String? oldPath,
  final int? added,
  final int? removed,
  final bool isBinary = false,
  final bool isLarge = false,
  final GitSubmoduleStatus? submodule,
  final String? submoduleRoot,
}) {
  String get id => '${area.key}::$path';

  bool get isSubmoduleChild => submoduleRoot != null;

  bool get isExpandableSubmodule =>
      submodule != null &&
      !isSubmoduleChild &&
      (submodule!.commitChanged ||
          submodule!.trackedChanges ||
          submodule!.untrackedChanges) &&
      submodule!.inspectable;

  bool get isSubmoduleWorktreeOnly =>
      area == GitChangeArea.unstaged &&
      submodule != null &&
      !submodule!.commitChanged;

  bool get canStageFromParent =>
      !isSubmoduleChild &&
      area != GitChangeArea.staged &&
      !isSubmoduleWorktreeOnly;

  bool get canUnstageFromParent =>
      !isSubmoduleChild && area == GitChangeArea.staged;

  bool get canDiscardFromParent =>
      !isSubmoduleChild &&
      area != GitChangeArea.staged &&
      !isSubmoduleWorktreeOnly &&
      (submodule == null ||
          (submodule!.inspectable &&
              !submodule!.trackedChanges &&
              !submodule!.untrackedChanges));

  GitChangeEntry insideSubmodule(String root) {
    final prefixedOldPath = oldPath == null ? null : '$root/$oldPath';
    return GitChangeEntry(
      path: '$root/$path',
      oldPath: prefixedOldPath,
      area: area,
      status: status,
      added: added,
      removed: removed,
      isBinary: isBinary,
      isLarge: isLarge,
      submodule: submodule,
      submoduleRoot: root,
    );
  }
}

class const GitSubmoduleStatus({
  required final bool commitChanged,
  required final bool trackedChanges,
  required final bool untrackedChanges,
  required final bool inspectable,
});

typedef _GitChangeEntryValueKey = ({
  String path,
  String? oldPath,
  GitChangeArea area,
  GitChangeStatus status,
  int? added,
  int? removed,
  bool isBinary,
  bool isLarge,
  bool? submoduleCommitChanged,
  bool? submoduleTrackedChanges,
  bool? submoduleUntrackedChanges,
  bool? submoduleInspectable,
  String? submoduleRoot,
});

bool gitSubmoduleStatusValuesEqual(
  GitSubmoduleStatus? left,
  GitSubmoduleStatus? right,
) {
  if (identical(left, right)) {
    return true;
  }
  if (left == null || right == null) {
    return false;
  }
  return left.commitChanged == right.commitChanged &&
      left.trackedChanges == right.trackedChanges &&
      left.untrackedChanges == right.untrackedChanges &&
      left.inspectable == right.inspectable;
}

bool gitChangeEntryValuesEqual(GitChangeEntry left, GitChangeEntry right) {
  if (identical(left, right)) {
    return true;
  }
  return _gitChangeEntryValueKey(left) == _gitChangeEntryValueKey(right);
}

List<GitChangeEntry> reconcileGitChangeEntryInstances(
  List<GitChangeEntry> previous,
  List<GitChangeEntry> next,
) {
  if (identical(previous, next)) {
    return previous;
  }

  final merged = List<GitChangeEntry>.of(next);
  final reusedPrevious = List<bool>.filled(previous.length, false);
  final reusedNext = List<bool>.filled(next.length, false);

  // Positional matching is the common case and avoids building an index.
  if (previous.length == next.length) {
    var allMatches = true;
    for (var index = 0; index < next.length; index += 1) {
      if (gitChangeEntryValuesEqual(previous[index], next[index])) {
        merged[index] = previous[index];
        reusedPrevious[index] = true;
        reusedNext[index] = true;
      } else {
        allMatches = false;
      }
    }
    if (allMatches) {
      return previous;
    }
  }

  final previousByValue = <_GitChangeEntryValueKey, List<GitChangeEntry>>{};
  for (var index = 0; index < previous.length; index += 1) {
    if (reusedPrevious[index]) {
      continue;
    }
    final entry = previous[index];
    (previousByValue[_gitChangeEntryValueKey(entry)] ??= <GitChangeEntry>[])
        .add(entry);
  }

  for (var index = 0; index < next.length; index += 1) {
    if (reusedNext[index]) {
      continue;
    }
    final candidates = previousByValue[_gitChangeEntryValueKey(next[index])];
    if (candidates != null && candidates.isNotEmpty) {
      merged[index] = candidates.removeLast();
    }
  }

  return List<GitChangeEntry>.unmodifiableOf(merged);
}

Future<List<GitChangeEntry>> reconcileGitChangeEntryInstancesChunked(
  List<GitChangeEntry> previous,
  List<GitChangeEntry> next, {
  int chunkSize = gitStatusWorkChunkSize,
  void Function(double milliseconds)? onChunk,
}) async {
  _validateGitStatusChunkSize(chunkSize);
  if (identical(previous, next)) {
    return previous;
  }

  final chunker = _GitStatusChunker(onChunk, chunkSize);
  try {
    final merged = List<GitChangeEntry>.of(next);
    final reusedPrevious = List<bool>.filled(previous.length, false);
    final reusedNext = List<bool>.filled(next.length, false);

    // Positional matching is the common case and avoids building an index.
    if (previous.length == next.length) {
      var allMatches = true;
      for (var index = 0; index < next.length; index += 1) {
        if (gitChangeEntryValuesEqual(previous[index], next[index])) {
          merged[index] = previous[index];
          reusedPrevious[index] = true;
          reusedNext[index] = true;
        } else {
          allMatches = false;
        }
        if ((index + 1) % chunkSize == 0) {
          await chunker.pause();
        }
      }
      if (allMatches) {
        return previous;
      }
    }

    final previousByValue = <_GitChangeEntryValueKey, List<GitChangeEntry>>{};
    for (var index = 0; index < previous.length; index += 1) {
      if (!reusedPrevious[index]) {
        final entry = previous[index];
        (previousByValue[_gitChangeEntryValueKey(entry)] ??= <GitChangeEntry>[])
            .add(entry);
      }
      if ((index + 1) % chunkSize == 0) {
        await chunker.pause();
      }
    }

    for (var index = 0; index < next.length; index += 1) {
      if (!reusedNext[index]) {
        final candidates =
            previousByValue[_gitChangeEntryValueKey(next[index])];
        if (candidates != null && candidates.isNotEmpty) {
          merged[index] = candidates.removeLast();
        }
      }
      if ((index + 1) % chunkSize == 0) {
        await chunker.pause();
      }
    }

    return List<GitChangeEntry>.unmodifiableOf(merged);
  } finally {
    chunker.finish();
  }
}

bool gitRepositoryStateValuesEqual(
  GitRepositoryState left,
  GitRepositoryState right,
) {
  if (identical(left, right)) {
    return true;
  }
  return left.branch == right.branch &&
      left.upstream == right.upstream &&
      left.ahead == right.ahead &&
      left.behind == right.behind &&
      left.hasConflicts == right.hasConflicts &&
      left.headMessage == right.headMessage;
}

bool gitStashEntryValuesEqual(GitStashEntry left, GitStashEntry right) {
  if (identical(left, right)) {
    return true;
  }
  return left.index == right.index &&
      left.reference == right.reference &&
      left.message == right.message &&
      left.oid == right.oid;
}

bool gitStashEntriesValuesEqual(
  List<GitStashEntry> left,
  List<GitStashEntry> right,
) {
  if (identical(left, right)) {
    return true;
  }
  if (left.length != right.length) {
    return false;
  }
  for (var index = 0; index < left.length; index += 1) {
    if (!gitStashEntryValuesEqual(left[index], right[index])) {
      return false;
    }
  }
  return true;
}

_GitChangeEntryValueKey _gitChangeEntryValueKey(GitChangeEntry entry) {
  final submodule = entry.submodule;
  return (
    path: entry.path,
    oldPath: entry.oldPath,
    area: entry.area,
    status: entry.status,
    added: entry.added,
    removed: entry.removed,
    isBinary: entry.isBinary,
    isLarge: entry.isLarge,
    submoduleCommitChanged: submodule?.commitChanged,
    submoduleTrackedChanges: submodule?.trackedChanges,
    submoduleUntrackedChanges: submodule?.untrackedChanges,
    submoduleInspectable: submodule?.inspectable,
    submoduleRoot: entry.submoduleRoot,
  );
}

class const GitChangeTreeRow({
  required final GitChangeTreeRowKind kind,
  required final String name,
  required final String path,
  required final int depth,
  required final int fileCount,
  final GitChangeEntry? entry,
});

class const GitDiffResult({
  required final List<GitDiffFile> files,
  final bool truncated = false,
});

class const GitDiffPage({
  required final List<GitDiffFile> files,
  final bool truncated = false,
});

class const GitDiffFile({
  required final String path,
  required final GitChangeArea area,
  required final GitChangeStatus status,
  final List<GitDiffLine> lines = const [],
  final String? oldPath,
  final int? added,
  final int? removed,
  final bool isBinary = false,
  final bool isLarge = false,
  final bool isGitlink = false,
  final bool truncated = false,
  final bool linePreviewTruncated = false,
  final String? sourceLabel,
});

class const GitDiffLine({
  required final String text,
  required final GitDiffLineKind kind,
}) {
  const new addition(String text) : this(text: text, kind: .addition);

  const new deletion(String text) : this(text: text, kind: .deletion);

  const new hunk(String text) : this(text: text, kind: .hunk);

  const new header(String text) : this(text: text, kind: .header);

  const new context(String text) : this(text: text, kind: .context);
}
