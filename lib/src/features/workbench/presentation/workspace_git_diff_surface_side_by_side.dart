part of 'workspace_git_diff_surface.dart';

final class _DiffSideLine {
  const _DiffSideLine({
    required this.lineNumber,
    required this.text,
    required this.kind,
  });

  final int? lineNumber;
  final String text;
  final GitDiffLineKind kind;
}

class const _SideBySideHeaderRow({
  required final String oldTitle,
  required final String newTitle,
}) extends _DiffRow {
  @override
  Widget build(BuildContext context) {
    final style = Theme.of(context).textTheme.labelSmall?.copyWith(
      color: AleraTokens.foregroundMuted,
      fontFamily: 'JetBrains Mono',
    );
    return DecoratedBox(
      decoration: const BoxDecoration(
        color: AleraTokens.surfaceVariant,
        border: Border(bottom: BorderSide(color: AleraTokens.borderSubtle)),
      ),
      child: Row(
        children: <Widget>[
          Expanded(
            child: Container(
              decoration: const BoxDecoration(
                border: Border(
                  right: BorderSide(color: AleraTokens.borderSubtle),
                ),
              ),
              padding: const EdgeInsets.symmetric(
                horizontal: AleraTokens.space12,
                vertical: AleraTokens.space4,
              ),
              child: Text(
                context.tr(oldTitle),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: style,
              ),
            ),
          ),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: AleraTokens.space12,
                vertical: AleraTokens.space4,
              ),
              child: Text(
                context.tr(newTitle),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: style,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class const _SideBySideDiffRow({
  required final _DiffSideLine? left,
  required final _DiffSideLine? right,
}) extends _DiffRow {
  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: <Widget>[
        Expanded(child: _SideBySideCell(line: left, isLeft: true)),
        Expanded(child: _SideBySideCell(line: right, isLeft: false)),
      ],
    );
  }
}

class const _SideBySideCell({
  required final _DiffSideLine? line,
  required final bool isLeft,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final kind = line?.kind;
    final (color, background, marker) = switch (kind) {
      GitDiffLineKind.addition => (
        AleraTokens.success,
        AleraTokens.success.withValues(alpha: 0.08),
        '+',
      ),
      GitDiffLineKind.deletion => (
        AleraTokens.error,
        AleraTokens.error.withValues(alpha: 0.08),
        '-',
      ),
      GitDiffLineKind.context => (
        AleraTokens.foregroundMuted,
        Colors.transparent,
        ' ',
      ),
      _ => (AleraTokens.foregroundMuted, Colors.transparent, ''),
    };
    return ClipRect(
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: background,
          border: isLeft
              ? const Border(right: BorderSide(color: AleraTokens.borderSubtle))
              : null,
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: AleraTokens.space8,
            vertical: AleraTokens.space2,
          ),
          child: Row(
            children: <Widget>[
              SizedBox(
                width: 36,
                child: Text(
                  line?.lineNumber?.toString() ?? '',
                  textAlign: TextAlign.right,
                  style: AleraTokens.monoStyle.copyWith(
                    fontSize: 11,
                    color: AleraTokens.foregroundMuted.withValues(alpha: 0.5),
                  ),
                ),
              ),
              const SizedBox(width: AleraTokens.space8),
              SizedBox(
                width: 12,
                child: Text(
                  marker,
                  textAlign: TextAlign.center,
                  style: AleraTokens.monoStyle.copyWith(
                    fontSize: 12,
                    color: color,
                  ),
                ),
              ),
              const SizedBox(width: AleraTokens.space6),
              Expanded(
                child: Text(
                  line?.text ?? '',
                  maxLines: 1,
                  overflow: TextOverflow.visible,
                  softWrap: false,
                  style: AleraTokens.monoStyle.copyWith(
                    fontSize: 12,
                    color: color,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

final RegExp _hunkHeaderRegExp = RegExp(
  r'^@@\s+-(\d+)(?:,(\d+))?\s+\+(\d+)(?:,(\d+))?\s+@@',
);

String _extractContent(String text) {
  if (text.isEmpty) {
    return '';
  }
  final first = text[0];
  if (first == '+' || first == '-' || first == ' ') {
    return text.substring(1);
  }
  return text;
}

List<GitDiffLine> _decodedDiffLines(
  GitDiffFile file,
  _FullFileContents? contents,
) {
  if (contents == null ||
      (contents.oldDecoded == null && contents.newDecoded == null)) {
    return file.lines;
  }
  final oldLines = contents.oldDecoded == null
      ? const <String>[]
      : _splitFullFileLines(contents.oldDecoded!.content);
  final newLines = contents.newDecoded == null
      ? const <String>[]
      : _splitFullFileLines(contents.newDecoded!.content);
  final result = <GitDiffLine>[];
  int? oldLine;
  int? newLine;

  String contentAt(List<String> lines, int? lineNumber, String fallback) {
    if (lineNumber == null || lineNumber <= 0 || lineNumber > lines.length) {
      return fallback;
    }
    return lines[lineNumber - 1];
  }

  for (final line in file.lines) {
    if (line.kind == GitDiffLineKind.hunk) {
      final match = _hunkHeaderRegExp.firstMatch(line.text);
      oldLine = int.tryParse(match?.group(1) ?? '');
      newLine = int.tryParse(match?.group(3) ?? '');
      result.add(line);
      continue;
    }
    if (line.kind == GitDiffLineKind.header) {
      result.add(line);
      continue;
    }
    final fallback = _extractContent(line.text);
    if (line.kind == GitDiffLineKind.deletion) {
      final content = contentAt(oldLines, oldLine, fallback);
      result.add(GitDiffLine.deletion('-$content'));
      if (oldLine != null) oldLine += 1;
      continue;
    }
    if (line.kind == GitDiffLineKind.addition) {
      final content = contentAt(newLines, newLine, fallback);
      result.add(GitDiffLine.addition('+$content'));
      if (newLine != null) newLine += 1;
      continue;
    }
    final content = newLines.isNotEmpty
        ? contentAt(newLines, newLine, fallback)
        : contentAt(oldLines, oldLine, fallback);
    result.add(GitDiffLine.context(' $content'));
    if (oldLine != null) oldLine += 1;
    if (newLine != null) newLine += 1;
  }
  return result;
}

GitDiffFile _fileWithDecodedDiffLines(
  GitDiffFile file,
  _FullFileContents? contents,
) {
  final lines = _decodedDiffLines(file, contents);
  if (identical(lines, file.lines)) return file;
  return GitDiffFile(
    path: file.path,
    area: file.area,
    status: file.status,
    lines: lines,
    oldPath: file.oldPath,
    added: file.added,
    removed: file.removed,
    isBinary: file.isBinary,
    isLarge: file.isLarge,
    isGitlink: file.isGitlink,
    truncated: file.truncated,
    linePreviewTruncated: file.linePreviewTruncated,
    sourceLabel: file.sourceLabel,
    sideBySideRows: file.sideBySideRows,
    fullFileRows: file.fullFileRows,
    fullFileSideBySideRows: file.fullFileSideBySideRows,
  );
}

class _ProjectedSideBySideDiffRowSpan implements _DiffRowSpan {
  const _ProjectedSideBySideDiffRowSpan(this.file);

  final GitDiffFile file;

  @override
  int get length => file.sideBySideRows.length;

  @override
  _DiffRow rowAt(int index) {
    final projection = file.sideBySideRows[index];
    return switch (projection.kind) {
      GitDiffSideBySideRowKind.passthrough => _DiffLineRow(
        file.lines[projection.lineIndex!],
      ),
      GitDiffSideBySideRowKind.pair => _SideBySideDiffRow(
        left: _projectedSideLine(
          file,
          projection.leftLineIndex,
          projection.leftLineNumber,
        ),
        right: _projectedSideLine(
          file,
          projection.rightLineIndex,
          projection.rightLineNumber,
        ),
      ),
    };
  }
}

List<_DiffRowSpan>? _buildProjectedSideBySideRowSpans(GitDiffFile file) {
  if (file.sideBySideRows.isEmpty) return null;
  for (final projection in file.sideBySideRows) {
    switch (projection.kind) {
      case GitDiffSideBySideRowKind.passthrough:
        final lineIndex = projection.lineIndex;
        if (lineIndex == null ||
            lineIndex < 0 ||
            lineIndex >= file.lines.length) {
          return null;
        }
      case GitDiffSideBySideRowKind.pair:
        for (final lineIndex in <int?>[
          projection.leftLineIndex,
          projection.rightLineIndex,
        ]) {
          if (lineIndex != null &&
              (lineIndex < 0 || lineIndex >= file.lines.length)) {
            return null;
          }
        }
    }
  }
  return <_DiffRowSpan>[
    _MaterializedDiffRowSpan(<_DiffRow>[
      _SideBySideHeaderRow(
        oldTitle: _diffOnlyOldLabel(file),
        newTitle: _diffOnlyNewLabel(file),
      ),
    ]),
    _ProjectedSideBySideDiffRowSpan(file),
  ];
}

_DiffSideLine? _projectedSideLine(
  GitDiffFile file,
  int? lineIndex,
  int? lineNumber,
) {
  if (lineIndex == null) return null;
  if (lineIndex < 0 || lineIndex >= file.lines.length) return null;
  final line = file.lines[lineIndex];
  return _DiffSideLine(
    lineNumber: lineNumber,
    text: _extractContent(line.text),
    kind: line.kind,
  );
}

String _diffOnlyOldLabel(GitDiffFile file) =>
    file.status == GitChangeStatus.added
    ? 'Empty'
    : (file.oldPath != null && file.oldPath != file.path
          ? 'Original (${file.oldPath})'
          : 'Original');

String _diffOnlyNewLabel(GitDiffFile file) =>
    file.status == GitChangeStatus.deleted ? 'Deleted' : 'Modified';

List<_DiffRow> _buildSideBySideRows(GitDiffFile file) {
  final items = <_DiffRow>[];
  final oldLabel = _diffOnlyOldLabel(file);
  final newLabel = _diffOnlyNewLabel(file);
  items.add(_SideBySideHeaderRow(oldTitle: oldLabel, newTitle: newLabel));

  int? currentOldLine;
  int? currentNewLine;

  final pendingDeletions = <_DiffSideLine>[];
  final pendingAdditions = <_DiffSideLine>[];

  void flushChanges() {
    if (pendingDeletions.isEmpty && pendingAdditions.isEmpty) {
      return;
    }
    final count = math.max(pendingDeletions.length, pendingAdditions.length);
    for (var i = 0; i < count; i++) {
      final left = i < pendingDeletions.length ? pendingDeletions[i] : null;
      final right = i < pendingAdditions.length ? pendingAdditions[i] : null;
      items.add(_SideBySideDiffRow(left: left, right: right));
    }
    pendingDeletions.clear();
    pendingAdditions.clear();
  }

  for (final line in file.lines) {
    if (line.kind == GitDiffLineKind.hunk) {
      flushChanges();
      final match = _hunkHeaderRegExp.firstMatch(line.text);
      if (match != null) {
        currentOldLine = int.tryParse(match.group(1) ?? '');
        if (match.group(2) == '0') {
          currentOldLine = null;
        }
        currentNewLine = int.tryParse(match.group(3) ?? '');
        if (match.group(4) == '0') {
          currentNewLine = null;
        }
      } else {
        currentOldLine = null;
        currentNewLine = null;
      }
      items.add(_DiffLineRow(line));
    } else if (line.kind == GitDiffLineKind.header) {
      flushChanges();
      items.add(_DiffLineRow(line));
    } else if (line.kind == GitDiffLineKind.deletion) {
      if (pendingAdditions.isNotEmpty) {
        flushChanges();
      }
      final lineNum = currentOldLine;
      if (currentOldLine != null) {
        currentOldLine = currentOldLine + 1;
      }
      pendingDeletions.add(
        _DiffSideLine(
          lineNumber: lineNum,
          text: _extractContent(line.text),
          kind: GitDiffLineKind.deletion,
        ),
      );
    } else if (line.kind == GitDiffLineKind.addition) {
      final lineNum = currentNewLine ?? (currentOldLine == null ? 1 : null);
      if (currentNewLine != null) {
        currentNewLine = currentNewLine + 1;
      } else if (currentOldLine == null) {
        currentNewLine = 2;
      }
      pendingAdditions.add(
        _DiffSideLine(
          lineNumber: lineNum,
          text: _extractContent(line.text),
          kind: GitDiffLineKind.addition,
        ),
      );
    } else if (line.kind == GitDiffLineKind.context) {
      flushChanges();
      final leftNum = currentOldLine;
      if (currentOldLine != null) {
        currentOldLine = currentOldLine + 1;
      }
      final rightNum = currentNewLine;
      if (currentNewLine != null) {
        currentNewLine = currentNewLine + 1;
      }
      final content = _extractContent(line.text);
      items.add(
        _SideBySideDiffRow(
          left: _DiffSideLine(
            lineNumber: leftNum,
            text: content,
            kind: GitDiffLineKind.context,
          ),
          right: _DiffSideLine(
            lineNumber: rightNum,
            text: content,
            kind: GitDiffLineKind.context,
          ),
        ),
      );
    }
  }

  flushChanges();
  return items;
}
