part of 'rust_git_backend.dart';

extension on RustGitBackend {
  GitChangeArea _toArea(rust.GitChangeArea area) {
    return switch (area) {
      rust.GitChangeArea.untracked => GitChangeArea.untracked,
      rust.GitChangeArea.unstaged => GitChangeArea.unstaged,
      rust.GitChangeArea.staged => GitChangeArea.staged,
    };
  }

  GitChangeTreeRowKind _toTreeRowKind(rust.GitChangeTreeRowKind kind) {
    return switch (kind) {
      rust.GitChangeTreeRowKind.directory => GitChangeTreeRowKind.directory,
      rust.GitChangeTreeRowKind.file => GitChangeTreeRowKind.file,
    };
  }

  rust.GitChangeArea _toRustArea(GitChangeArea area) {
    return switch (area) {
      GitChangeArea.untracked => rust.GitChangeArea.untracked,
      GitChangeArea.unstaged => rust.GitChangeArea.unstaged,
      GitChangeArea.staged => rust.GitChangeArea.staged,
    };
  }

  rust_commit_ops.GitResetMode _toRustResetMode(GitResetMode mode) {
    return switch (mode) {
      GitResetMode.soft => rust_commit_ops.GitResetMode.soft,
      GitResetMode.mixed => rust_commit_ops.GitResetMode.mixed,
      GitResetMode.hard => rust_commit_ops.GitResetMode.hard,
    };
  }

  GitExplorerStatusSnapshot _toExplorerStatusSnapshot(
    explorer_rust.GitExplorerStatusSnapshot snapshot,
  ) {
    return GitExplorerStatusSnapshot(<String, GitExplorerStatus>{
      for (final entry in snapshot.entries)
        entry.path: switch (entry.status) {
          explorer_rust.GitExplorerStatus.untracked =>
            GitExplorerStatus.untracked,
          explorer_rust.GitExplorerStatus.added => GitExplorerStatus.added,
          explorer_rust.GitExplorerStatus.modified =>
            GitExplorerStatus.modified,
        },
    });
  }

  GitDiffLine _toDiffLine(rust.GitDiffLine line) {
    return GitDiffLine(text: line.text, kind: _toDiffLineKind(line.kind));
  }

  GitDiffLineKind _toDiffLineKind(rust.GitDiffLineKind kind) {
    return switch (kind) {
      rust.GitDiffLineKind.addition => GitDiffLineKind.addition,
      rust.GitDiffLineKind.deletion => GitDiffLineKind.deletion,
      rust.GitDiffLineKind.hunk => GitDiffLineKind.hunk,
      rust.GitDiffLineKind.header => GitDiffLineKind.header,
      rust.GitDiffLineKind.context => GitDiffLineKind.context,
    };
  }

  GitDiffSideBySideRowKind _toDiffSideBySideRowKind(
    rust.GitDiffSideBySideRowKind kind,
  ) {
    return switch (kind) {
      rust.GitDiffSideBySideRowKind.passthrough =>
        GitDiffSideBySideRowKind.passthrough,
      rust.GitDiffSideBySideRowKind.pair => GitDiffSideBySideRowKind.pair,
    };
  }

  GitDiffFullFileRowKind _toDiffFullFileRowKind(
    rust.GitDiffFullFileRowKind kind,
  ) {
    return switch (kind) {
      rust.GitDiffFullFileRowKind.contextRange =>
        GitDiffFullFileRowKind.contextRange,
      rust.GitDiffFullFileRowKind.line => GitDiffFullFileRowKind.line,
    };
  }

  GitDiffFullFileSideBySideRowKind _toDiffFullFileSideBySideRowKind(
    rust.GitDiffFullFileSideBySideRowKind kind,
  ) {
    return switch (kind) {
      rust.GitDiffFullFileSideBySideRowKind.contextRange =>
        GitDiffFullFileSideBySideRowKind.contextRange,
      rust.GitDiffFullFileSideBySideRowKind.pair =>
        GitDiffFullFileSideBySideRowKind.pair,
    };
  }

  GitHistoryRefCategory _toHistoryRefCategory(
    rust.GitHistoryRefCategory category,
  ) {
    return switch (category) {
      rust.GitHistoryRefCategory.branches => GitHistoryRefCategory.branches,
      rust.GitHistoryRefCategory.remoteBranches =>
        GitHistoryRefCategory.remoteBranches,
      rust.GitHistoryRefCategory.tags => GitHistoryRefCategory.tags,
      rust.GitHistoryRefCategory.commits => GitHistoryRefCategory.commits,
    };
  }

  GitCommitCompareStatus _toCommitCompareStatus(
    rust.GitCommitCompareStatus status,
  ) {
    return switch (status) {
      rust.GitCommitCompareStatus.ready => GitCommitCompareStatus.ready,
      rust.GitCommitCompareStatus.invalidCommit =>
        GitCommitCompareStatus.invalidCommit,
      rust.GitCommitCompareStatus.error => GitCommitCompareStatus.error,
    };
  }

  GitChangeStatus _toStatus(rust.GitChangeStatus status) {
    return switch (status) {
      rust.GitChangeStatus.modified => GitChangeStatus.modified,
      rust.GitChangeStatus.added => GitChangeStatus.added,
      rust.GitChangeStatus.deleted => GitChangeStatus.deleted,
      rust.GitChangeStatus.renamed => GitChangeStatus.renamed,
      rust.GitChangeStatus.copied => GitChangeStatus.copied,
      rust.GitChangeStatus.untracked => GitChangeStatus.untracked,
    };
  }

  GitDiffResult _toDiffResult(
    rust.GitDiffResult result, {
    String? sourceLabel,
  }) {
    return GitDiffResult(
      truncated: result.truncated,
      files: result.files
          .map((file) => _toDiffFile(file, sourceLabel: sourceLabel))
          .toList(growable: false),
    );
  }

  GitDiffPage _toDiffPage(rust.GitDiffPage result) {
    return GitDiffPage(
      truncated: result.truncated,
      files: result.files
          .map((file) => _toDiffFile(file))
          .toList(growable: false),
    );
  }

  GitDiffFile _toDiffFile(rust.GitDiffFile file, {String? sourceLabel}) {
    return GitDiffFile(
      path: file.path,
      oldPath: file.oldPath,
      area: _toArea(file.area),
      status: _toStatus(file.status),
      lines: file.lines.map(_toDiffLine).toList(growable: false),
      sideBySideRows: file.sideBySideRows
          .map(
            (row) => GitDiffSideBySideRow(
              kind: _toDiffSideBySideRowKind(row.kind),
              lineIndex: row.lineIndex,
              leftLineIndex: row.leftLineIndex,
              leftLineNumber: row.leftLineNumber,
              rightLineIndex: row.rightLineIndex,
              rightLineNumber: row.rightLineNumber,
            ),
          )
          .toList(growable: false),
      fullFileRows: file.fullFileRows
          .map(
            (row) => GitDiffFullFileRow(
              kind: _toDiffFullFileRowKind(row.kind),
              startIndex: row.startIndex,
              endIndex: row.endIndex,
              fullLineIndex: row.fullLineIndex,
              diffLineIndex: row.diffLineIndex,
              lineNumber: row.lineNumber,
            ),
          )
          .toList(growable: false),
      fullFileSideBySideRows: file.fullFileSideBySideRows
          .map(
            (row) => GitDiffFullFileSideBySideRow(
              kind: _toDiffFullFileSideBySideRowKind(row.kind),
              oldStartIndex: row.oldStartIndex,
              oldEndIndex: row.oldEndIndex,
              newStartIndex: row.newStartIndex,
              newEndIndex: row.newEndIndex,
              leftDiffLineIndex: row.leftDiffLineIndex,
              rightDiffLineIndex: row.rightDiffLineIndex,
            ),
          )
          .toList(growable: false),
      added: file.added,
      removed: file.removed,
      isBinary: file.isBinary,
      isLarge: file.isLarge,
      isGitlink: file.isGitlink,
      truncated: file.truncated,
      linePreviewTruncated: file.linePreviewTruncated,
      sourceLabel: sourceLabel,
    );
  }
}

rust_archive_ops.GitArchiveFormat _toRustArchiveFormat(
  GitArchiveFormat format,
) {
  return switch (format) {
    GitArchiveFormat.zip => rust_archive_ops.GitArchiveFormat.zip,
    GitArchiveFormat.tar => rust_archive_ops.GitArchiveFormat.tar,
  };
}
