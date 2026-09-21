part of 'workspace_git_diff_surface.dart';

final class _FullFileContents {
  const _FullFileContents({
    this.oldBytes,
    this.newBytes,
    this.oldDecoded,
    this.newDecoded,
  });

  final Uint8List? oldBytes;
  final Uint8List? newBytes;
  final native.WorkspaceDecodedText? oldDecoded;
  final native.WorkspaceDecodedText? newDecoded;

  native.WorkspaceDecodedText? singleSideDecoded(GitDiffFile file) =>
      file.status == GitChangeStatus.deleted ? oldDecoded : newDecoded;
}

final class _FullFileLine {
  const _FullFileLine({
    required this.lineNumber,
    required this.text,
    required this.kind,
  });

  final int? lineNumber;
  final String text;
  final GitDiffLineKind kind;
}

class const _FullFileDiffRow(final _FullFileLine line) extends _DiffRow {
  @override
  Widget build(BuildContext context) => _FullFileDiffLine(line: line);
}

class const _FullFileDiffLine({required final _FullFileLine line})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final (color, background, marker) = switch (line.kind) {
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
      _ => (AleraTokens.foregroundMuted, Colors.transparent, ' '),
    };
    final syntax = _DiffSyntaxScope.maybeOf(context);
    final codeStyle =
        syntax?.textStyle ??
        AleraTokens.monoStyle.copyWith(
          fontSize: 12,
          color: AleraTokens.foreground,
        );
    return DecoratedBox(
      decoration: BoxDecoration(color: background),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: AleraTokens.space8,
          vertical: AleraTokens.space2,
        ),
        child: Row(
          children: <Widget>[
            SizedBox(
              width: 42,
              child: SelectionContainer.disabled(
                child: Text(
                  line.lineNumber?.toString() ?? '',
                  textAlign: TextAlign.right,
                  style: AleraTokens.monoStyle.copyWith(
                    fontSize: 11,
                    color: AleraTokens.foregroundMuted.withValues(alpha: 0.5),
                  ),
                ),
              ),
            ),
            const SizedBox(width: AleraTokens.space8),
            SizedBox(
              width: 12,
              child: SelectionContainer.disabled(
                child: Text(
                  marker,
                  textAlign: TextAlign.center,
                  style: AleraTokens.monoStyle.copyWith(
                    fontSize: 12,
                    color: color,
                  ),
                ),
              ),
            ),
            const SizedBox(width: AleraTokens.space6),
            Expanded(
              child: Text.rich(
                syntax?.lineSpan(
                      line.text,
                      lineIndex: math.max((line.lineNumber ?? 1) - 1, 0),
                    ) ??
                    TextSpan(text: line.text, style: codeStyle),
                maxLines: 1,
                overflow: TextOverflow.visible,
                softWrap: false,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _FullFileForcedDiffRowSpan implements _DiffRowSpan {
  const _FullFileForcedDiffRowSpan({required this.lines, required this.kind});

  final List<String> lines;
  final GitDiffLineKind kind;

  @override
  int get length => lines.length;

  @override
  _DiffRow rowAt(int index) => _FullFileDiffRow(
    _FullFileLine(lineNumber: index + 1, text: lines[index], kind: kind),
  );
}

class _FullFileContextDiffRowSpan implements _DiffRowSpan {
  const _FullFileContextDiffRowSpan({
    required this.lines,
    required this.start,
    required this.end,
  });

  final List<String> lines;
  final int start;
  final int end;

  @override
  int get length => end - start;

  @override
  _DiffRow rowAt(int index) {
    final lineIndex = start + index;
    return _FullFileDiffRow(
      _FullFileLine(
        lineNumber: lineIndex + 1,
        text: lines[lineIndex],
        kind: GitDiffLineKind.context,
      ),
    );
  }
}

List<_DiffRowSpan>? _buildProjectedFullFileRowSpans(
  GitDiffFile file,
  native.WorkspaceDecodedText? decoded,
) {
  final lines = _decodeFullFileLines(decoded);
  if (lines == null) return null;
  final forceKind = switch (file.status) {
    GitChangeStatus.deleted => GitDiffLineKind.deletion,
    GitChangeStatus.added ||
    GitChangeStatus.untracked => GitDiffLineKind.addition,
    _ => null,
  };
  if (forceKind != null) {
    return <_DiffRowSpan>[
      if (lines.isNotEmpty)
        _FullFileForcedDiffRowSpan(lines: lines, kind: forceKind),
    ];
  }
  if (file.fullFileRows.isEmpty) return null;
  final spans = <_DiffRowSpan>[];
  final materialized = <_DiffRow>[];

  void flushMaterialized() {
    if (materialized.isEmpty) return;
    spans.add(
      _MaterializedDiffRowSpan(List<_DiffRow>.unmodifiable(materialized)),
    );
    materialized.clear();
  }

  for (final projection in file.fullFileRows) {
    switch (projection.kind) {
      case GitDiffFullFileRowKind.contextRange:
        final start = projection.startIndex;
        if (start == null || start < 0 || start > lines.length) return null;
        final end = projection.endIndex ?? lines.length;
        if (end < start || end > lines.length) return null;
        flushMaterialized();
        if (end > start) {
          spans.add(
            _FullFileContextDiffRowSpan(lines: lines, start: start, end: end),
          );
        }
      case GitDiffFullFileRowKind.line:
        final diffLineIndex = projection.diffLineIndex;
        if (diffLineIndex == null ||
            diffLineIndex < 0 ||
            diffLineIndex >= file.lines.length) {
          return null;
        }
        final diffLine = file.lines[diffLineIndex];
        final fullLineIndex = projection.fullLineIndex;
        final text =
            diffLine.kind == GitDiffLineKind.context &&
                fullLineIndex != null &&
                fullLineIndex >= 0 &&
                fullLineIndex < lines.length
            ? lines[fullLineIndex]
            : _extractContent(diffLine.text);
        materialized.add(
          _FullFileDiffRow(
            _FullFileLine(
              lineNumber: projection.lineNumber,
              text: text,
              kind: diffLine.kind,
            ),
          ),
        );
    }
  }
  flushMaterialized();
  return spans;
}

List<_DiffRow>? _buildFullFileRows(
  GitDiffFile file,
  native.WorkspaceDecodedText? decoded,
) {
  final lines = _decodeFullFileLines(decoded);
  if (lines == null) return null;
  final forceKind = switch (file.status) {
    GitChangeStatus.deleted => GitDiffLineKind.deletion,
    GitChangeStatus.added ||
    GitChangeStatus.untracked => GitDiffLineKind.addition,
    _ => null,
  };
  if (forceKind != null) {
    return <_DiffRow>[
      for (var i = 0; i < lines.length; i++)
        _FullFileDiffRow(
          _FullFileLine(lineNumber: i + 1, text: lines[i], kind: forceKind),
        ),
    ];
  }

  final rows = <_DiffRow>[];
  var nextFullIndex = 0;
  int? oldLine;
  int? newLine;

  void appendUnchangedUntil(int exclusiveLineNumber) {
    final targetIndex = math.min(
      math.max(exclusiveLineNumber - 1, 0),
      lines.length,
    );
    while (nextFullIndex < targetIndex) {
      rows.add(
        _FullFileDiffRow(
          _FullFileLine(
            lineNumber: nextFullIndex + 1,
            text: lines[nextFullIndex],
            kind: GitDiffLineKind.context,
          ),
        ),
      );
      nextFullIndex += 1;
    }
  }

  for (final diffLine in file.lines) {
    if (diffLine.kind == GitDiffLineKind.hunk) {
      final match = _hunkHeaderRegExp.firstMatch(diffLine.text);
      if (match == null) continue;
      oldLine = int.tryParse(match.group(1) ?? '');
      newLine = int.tryParse(match.group(3) ?? '');
      appendUnchangedUntil(newLine ?? (nextFullIndex + 1));
      continue;
    }
    if (diffLine.kind == GitDiffLineKind.header) continue;
    if (diffLine.kind == GitDiffLineKind.deletion) {
      rows.add(
        _FullFileDiffRow(
          _FullFileLine(
            lineNumber: oldLine,
            text: _extractContent(diffLine.text),
            kind: GitDiffLineKind.deletion,
          ),
        ),
      );
      if (oldLine != null) oldLine += 1;
      continue;
    }
    final lineNumber = newLine ?? (nextFullIndex + 1);
    if (diffLine.kind == GitDiffLineKind.addition) {
      rows.add(
        _FullFileDiffRow(
          _FullFileLine(
            lineNumber: lineNumber,
            text: _extractContent(diffLine.text),
            kind: GitDiffLineKind.addition,
          ),
        ),
      );
    } else if (diffLine.kind == GitDiffLineKind.context) {
      rows.add(
        _FullFileDiffRow(
          _FullFileLine(
            lineNumber: lineNumber,
            text: nextFullIndex < lines.length
                ? lines[nextFullIndex]
                : _extractContent(diffLine.text),
            kind: GitDiffLineKind.context,
          ),
        ),
      );
      if (oldLine != null) oldLine += 1;
    }
    if (nextFullIndex < lines.length) nextFullIndex += 1;
    newLine = lineNumber + 1;
  }
  appendUnchangedUntil(lines.length + 1);
  return rows;
}

class _FullFileSideBySideForcedDiffRowSpan implements _DiffRowSpan {
  const _FullFileSideBySideForcedDiffRowSpan({
    required this.lines,
    required this.kind,
    required this.left,
  });

  final List<String> lines;
  final GitDiffLineKind kind;
  final bool left;

  @override
  int get length => lines.length;

  @override
  _DiffRow rowAt(int index) {
    final line = _DiffSideLine(
      lineNumber: index + 1,
      text: lines[index],
      kind: kind,
    );
    return _SideBySideDiffRow(
      left: left ? line : null,
      right: left ? null : line,
    );
  }
}

class _FullFileSideBySideContextDiffRowSpan implements _DiffRowSpan {
  const _FullFileSideBySideContextDiffRowSpan({
    required this.oldLines,
    required this.newLines,
    required this.oldStart,
    required this.oldEnd,
    required this.newStart,
    required this.newEnd,
  });

  final List<String> oldLines;
  final List<String> newLines;
  final int oldStart;
  final int oldEnd;
  final int newStart;
  final int newEnd;

  @override
  int get length => math.max(oldEnd - oldStart, newEnd - newStart);

  @override
  _DiffRow rowAt(int index) {
    final oldIndex = oldStart + index;
    final newIndex = newStart + index;
    final hasOld = oldIndex < oldEnd;
    final hasNew = newIndex < newEnd;
    return _SideBySideDiffRow(
      left: hasOld
          ? _DiffSideLine(
              lineNumber: oldIndex + 1,
              text: oldLines[oldIndex],
              kind: GitDiffLineKind.context,
            )
          : null,
      right: hasNew
          ? _DiffSideLine(
              lineNumber: newIndex + 1,
              text: newLines[newIndex],
              kind: GitDiffLineKind.context,
            )
          : null,
    );
  }
}

List<_DiffRowSpan>? _buildProjectedFullFileSideBySideRowSpans(
  GitDiffFile file,
  _FullFileContents? contents,
) {
  if (contents == null) return null;
  final header = _MaterializedDiffRowSpan(<_DiffRow>[
    _SideBySideHeaderRow(
      oldTitle: _fullFileOldLabel(file),
      newTitle: _fullFileNewLabel(file),
    ),
  ]);
  if (file.status == GitChangeStatus.added ||
      file.status == GitChangeStatus.untracked) {
    final newLines = _decodeFullFileLines(contents.newDecoded);
    if (newLines == null) return null;
    return <_DiffRowSpan>[
      header,
      if (newLines.isNotEmpty)
        _FullFileSideBySideForcedDiffRowSpan(
          lines: newLines,
          kind: GitDiffLineKind.addition,
          left: false,
        ),
    ];
  }
  if (file.status == GitChangeStatus.deleted) {
    final oldLines = _decodeFullFileLines(contents.oldDecoded);
    if (oldLines == null) return null;
    return <_DiffRowSpan>[
      header,
      if (oldLines.isNotEmpty)
        _FullFileSideBySideForcedDiffRowSpan(
          lines: oldLines,
          kind: GitDiffLineKind.deletion,
          left: true,
        ),
    ];
  }
  if (file.fullFileSideBySideRows.isEmpty) return null;
  final oldLines = _decodeFullFileLines(contents.oldDecoded);
  final newLines = _decodeFullFileLines(contents.newDecoded);
  if (oldLines == null || newLines == null) return null;

  final spans = <_DiffRowSpan>[header];
  final materialized = <_DiffRow>[];

  void flushMaterialized() {
    if (materialized.isEmpty) return;
    spans.add(
      _MaterializedDiffRowSpan(List<_DiffRow>.unmodifiable(materialized)),
    );
    materialized.clear();
  }

  for (final projection in file.fullFileSideBySideRows) {
    switch (projection.kind) {
      case GitDiffFullFileSideBySideRowKind.contextRange:
        final oldStart = projection.oldStartIndex;
        final newStart = projection.newStartIndex;
        if (oldStart == null || newStart == null) return null;
        final oldEnd = math.min(
          projection.oldEndIndex ?? oldLines.length,
          oldLines.length,
        );
        final newEnd = math.min(
          projection.newEndIndex ?? newLines.length,
          newLines.length,
        );
        if (oldStart < 0 ||
            newStart < 0 ||
            oldEnd < oldStart ||
            newEnd < newStart ||
            oldStart > oldLines.length ||
            newStart > newLines.length) {
          return null;
        }
        flushMaterialized();
        if (oldEnd > oldStart || newEnd > newStart) {
          spans.add(
            _FullFileSideBySideContextDiffRowSpan(
              oldLines: oldLines,
              newLines: newLines,
              oldStart: oldStart,
              oldEnd: oldEnd,
              newStart: newStart,
              newEnd: newEnd,
            ),
          );
        }
      case GitDiffFullFileSideBySideRowKind.pair:
        final left = _projectedFullFileSideLine(
          file: file,
          fullLines: oldLines,
          fullIndex: projection.oldStartIndex,
          diffLineIndex: projection.leftDiffLineIndex,
        );
        final right = _projectedFullFileSideLine(
          file: file,
          fullLines: newLines,
          fullIndex: projection.newStartIndex,
          diffLineIndex: projection.rightDiffLineIndex,
        );
        if ((projection.oldStartIndex != null ||
                projection.leftDiffLineIndex != null) &&
            left == null) {
          return null;
        }
        if ((projection.newStartIndex != null ||
                projection.rightDiffLineIndex != null) &&
            right == null) {
          return null;
        }
        materialized.add(_SideBySideDiffRow(left: left, right: right));
    }
  }
  flushMaterialized();
  return spans;
}

_DiffSideLine? _projectedFullFileSideLine({
  required GitDiffFile file,
  required List<String> fullLines,
  required int? fullIndex,
  required int? diffLineIndex,
}) {
  if (fullIndex == null && diffLineIndex == null) return null;
  GitDiffLine? diffLine;
  if (diffLineIndex != null) {
    if (diffLineIndex < 0 || diffLineIndex >= file.lines.length) return null;
    diffLine = file.lines[diffLineIndex];
  }
  if (fullIndex != null && fullIndex >= 0 && fullIndex < fullLines.length) {
    return _DiffSideLine(
      lineNumber: fullIndex + 1,
      text: fullLines[fullIndex],
      kind: diffLine?.kind ?? GitDiffLineKind.context,
    );
  }
  if (diffLine == null) return null;
  return _DiffSideLine(
    lineNumber: null,
    text: _extractContent(diffLine.text),
    kind: diffLine.kind,
  );
}

String _fullFileOldLabel(GitDiffFile file) =>
    file.status == GitChangeStatus.added ||
        file.status == GitChangeStatus.untracked
    ? 'Empty'
    : (file.oldPath != null && file.oldPath != file.path
          ? 'Original (${file.oldPath})'
          : 'Original');

String _fullFileNewLabel(GitDiffFile file) =>
    file.status == GitChangeStatus.deleted ? 'Deleted' : 'Modified';

List<_DiffRow>? _buildFullFileSideBySideRows(
  GitDiffFile file,
  _FullFileContents? contents,
) {
  if (contents == null) return null;
  final oldLines = _decodeFullFileLines(contents.oldDecoded);
  final newLines = _decodeFullFileLines(contents.newDecoded);
  final oldLabel = _fullFileOldLabel(file);
  final newLabel = _fullFileNewLabel(file);

  if (file.status == GitChangeStatus.added ||
      file.status == GitChangeStatus.untracked) {
    if (newLines == null) return null;
    return <_DiffRow>[
      _SideBySideHeaderRow(oldTitle: oldLabel, newTitle: newLabel),
      for (var i = 0; i < newLines.length; i++)
        _SideBySideDiffRow(
          left: null,
          right: _DiffSideLine(
            lineNumber: i + 1,
            text: newLines[i],
            kind: GitDiffLineKind.addition,
          ),
        ),
    ];
  }
  if (file.status == GitChangeStatus.deleted) {
    if (oldLines == null) return null;
    return <_DiffRow>[
      _SideBySideHeaderRow(oldTitle: oldLabel, newTitle: newLabel),
      for (var i = 0; i < oldLines.length; i++)
        _SideBySideDiffRow(
          left: _DiffSideLine(
            lineNumber: i + 1,
            text: oldLines[i],
            kind: GitDiffLineKind.deletion,
          ),
          right: null,
        ),
    ];
  }
  if (oldLines == null || newLines == null) return null;

  final rows = <_DiffRow>[
    _SideBySideHeaderRow(oldTitle: oldLabel, newTitle: newLabel),
  ];
  var oldIndex = 0;
  var newIndex = 0;
  final pendingDeletions = <_DiffSideLine>[];
  final pendingAdditions = <_DiffSideLine>[];

  void flushChanges() {
    if (pendingDeletions.isEmpty && pendingAdditions.isEmpty) return;
    final count = math.max(pendingDeletions.length, pendingAdditions.length);
    for (var i = 0; i < count; i++) {
      rows.add(
        _SideBySideDiffRow(
          left: i < pendingDeletions.length ? pendingDeletions[i] : null,
          right: i < pendingAdditions.length ? pendingAdditions[i] : null,
        ),
      );
    }
    pendingDeletions.clear();
    pendingAdditions.clear();
  }

  void appendContextUntil(int oldTarget, int newTarget) {
    flushChanges();
    while (oldIndex < oldTarget || newIndex < newTarget) {
      final hasOld = oldIndex < oldTarget && oldIndex < oldLines.length;
      final hasNew = newIndex < newTarget && newIndex < newLines.length;
      rows.add(
        _SideBySideDiffRow(
          left: hasOld
              ? _DiffSideLine(
                  lineNumber: oldIndex + 1,
                  text: oldLines[oldIndex],
                  kind: GitDiffLineKind.context,
                )
              : null,
          right: hasNew
              ? _DiffSideLine(
                  lineNumber: newIndex + 1,
                  text: newLines[newIndex],
                  kind: GitDiffLineKind.context,
                )
              : null,
        ),
      );
      if (hasOld) oldIndex += 1;
      if (hasNew) newIndex += 1;
      if (!hasOld && !hasNew) break;
    }
  }

  for (final diffLine in file.lines) {
    if (diffLine.kind == GitDiffLineKind.hunk) {
      final match = _hunkHeaderRegExp.firstMatch(diffLine.text);
      if (match == null) continue;
      final oldStart = int.tryParse(match.group(1) ?? '') ?? (oldIndex + 1);
      final newStart = int.tryParse(match.group(3) ?? '') ?? (newIndex + 1);
      appendContextUntil(
        math.max(oldStart - 1, oldIndex),
        math.max(newStart - 1, newIndex),
      );
      continue;
    }
    if (diffLine.kind == GitDiffLineKind.header) continue;
    if (diffLine.kind == GitDiffLineKind.deletion) {
      if (pendingAdditions.isNotEmpty) flushChanges();
      pendingDeletions.add(
        _DiffSideLine(
          lineNumber: oldIndex < oldLines.length ? oldIndex + 1 : null,
          text: oldIndex < oldLines.length
              ? oldLines[oldIndex]
              : _extractContent(diffLine.text),
          kind: GitDiffLineKind.deletion,
        ),
      );
      if (oldIndex < oldLines.length) oldIndex += 1;
      continue;
    }
    if (diffLine.kind == GitDiffLineKind.addition) {
      pendingAdditions.add(
        _DiffSideLine(
          lineNumber: newIndex < newLines.length ? newIndex + 1 : null,
          text: newIndex < newLines.length
              ? newLines[newIndex]
              : _extractContent(diffLine.text),
          kind: GitDiffLineKind.addition,
        ),
      );
      if (newIndex < newLines.length) newIndex += 1;
      continue;
    }
    if (diffLine.kind == GitDiffLineKind.context) {
      flushChanges();
      rows.add(
        _SideBySideDiffRow(
          left: _DiffSideLine(
            lineNumber: oldIndex < oldLines.length ? oldIndex + 1 : null,
            text: oldIndex < oldLines.length
                ? oldLines[oldIndex]
                : _extractContent(diffLine.text),
            kind: GitDiffLineKind.context,
          ),
          right: _DiffSideLine(
            lineNumber: newIndex < newLines.length ? newIndex + 1 : null,
            text: newIndex < newLines.length
                ? newLines[newIndex]
                : _extractContent(diffLine.text),
            kind: GitDiffLineKind.context,
          ),
        ),
      );
      if (oldIndex < oldLines.length) oldIndex += 1;
      if (newIndex < newLines.length) newIndex += 1;
    }
  }
  flushChanges();
  appendContextUntil(oldLines.length, newLines.length);
  return rows;
}

List<String>? _decodeFullFileLines(native.WorkspaceDecodedText? decoded) {
  if (decoded == null) return null;
  return _splitFullFileLines(decoded.content);
}

List<String> _splitFullFileLines(String text) {
  if (text.isEmpty) return const <String>[];
  final normalized = text.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
  final lines = normalized.split('\n');
  if (normalized.endsWith('\n')) lines.removeLast();
  return lines;
}
