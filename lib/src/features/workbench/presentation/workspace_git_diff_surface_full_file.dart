part of 'workspace_git_diff_surface.dart';

final class _FullFileContents {
  const _FullFileContents({this.oldBytes, this.newBytes});

  final Uint8List? oldBytes;
  final Uint8List? newBytes;

  Uint8List? singleSideBytes(GitDiffFile file) =>
      file.status == GitChangeStatus.deleted ? oldBytes : newBytes;
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
              child: Text(
                line.lineNumber?.toString() ?? '',
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
                line.text,
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
    );
  }
}

List<_DiffRow>? _buildFullFileRows(GitDiffFile file, Uint8List? bytes) {
  final lines = _decodeFullFileLines(bytes);
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

List<_DiffRow>? _buildFullFileSideBySideRows(
  GitDiffFile file,
  _FullFileContents? contents,
) {
  if (contents == null) return null;
  final oldLines = _decodeFullFileLines(contents.oldBytes);
  final newLines = _decodeFullFileLines(contents.newBytes);
  final oldLabel =
      file.status == GitChangeStatus.added ||
          file.status == GitChangeStatus.untracked
      ? 'Empty'
      : (file.oldPath != null && file.oldPath != file.path
            ? 'Original (${file.oldPath})'
            : 'Original');
  final newLabel = file.status == GitChangeStatus.deleted
      ? 'Deleted'
      : 'Modified';

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

List<String>? _decodeFullFileLines(Uint8List? bytes) {
  if (bytes == null) return null;
  try {
    return _splitFullFileLines(utf8.decode(bytes));
  } on FormatException {
    return null;
  }
}

List<String> _splitFullFileLines(String text) {
  if (text.isEmpty) return const <String>[];
  final normalized = text.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
  final lines = normalized.split('\n');
  if (normalized.endsWith('\n')) lines.removeLast();
  return lines;
}
