import 'dart:math' as math;

part 'git_range_models.dart';
part 'git_history_graph_models.dart';
part 'git_history_models.dart';
part 'git_change_group_models.dart';
part 'git_change_tree_models.dart';
part 'git_change_entry_models.dart';

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

enum GitDiffSideBySideRowKind { passthrough, pair }

class const GitDiffSideBySideRow({
  required final GitDiffSideBySideRowKind kind,
  final int? lineIndex,
  final int? leftLineIndex,
  final int? leftLineNumber,
  final int? rightLineIndex,
  final int? rightLineNumber,
});

enum GitDiffFullFileRowKind { contextRange, line }

class const GitDiffFullFileRow({
  required final GitDiffFullFileRowKind kind,
  final int? startIndex,
  final int? endIndex,
  final int? fullLineIndex,
  final int? diffLineIndex,
  final int? lineNumber,
});

enum GitDiffFullFileSideBySideRowKind { contextRange, pair }

class const GitDiffFullFileSideBySideRow({
  required final GitDiffFullFileSideBySideRowKind kind,
  final int? oldStartIndex,
  final int? oldEndIndex,
  final int? newStartIndex,
  final int? newEndIndex,
  final int? leftDiffLineIndex,
  final int? rightDiffLineIndex,
});

enum GitDiffWhitespaceMode {
  normal,
  ignoreEol,
  ignoreChanges,
  ignoreAll;

  String get label => switch (this) {
    GitDiffWhitespaceMode.normal => 'Normal',
    GitDiffWhitespaceMode.ignoreEol => 'Ignore End-of-Line Whitespace',
    GitDiffWhitespaceMode.ignoreChanges => 'Ignore Whitespace Changes',
    GitDiffWhitespaceMode.ignoreAll => 'Ignore All Whitespace',
  };
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

class const GitChangeTreeRow({
  required final GitChangeTreeRowKind kind,
  required final String name,
  required final String path,
  required final int depth,
  required final int fileCount,
  final GitChangeEntry? entry,
  final int? entryIndex,
});

class const GitDiffResult({
  required final List<GitDiffFile> files,
  final bool truncated = false,
});

class const GitDiffPage({
  required final List<GitDiffFile> files,
  final bool truncated = false,
});

class const GitFileRevision({
  required final String oid,
  required final String shortOid,
  required final String subject,
});

class const GitDiffFile({
  required final String path,
  required final GitChangeArea area,
  required final GitChangeStatus status,
  final List<GitDiffLine> lines = const [],
  final List<GitDiffSideBySideRow> sideBySideRows = const [],
  final List<GitDiffFullFileRow> fullFileRows = const [],
  final List<GitDiffFullFileSideBySideRow> fullFileSideBySideRows = const [],
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
