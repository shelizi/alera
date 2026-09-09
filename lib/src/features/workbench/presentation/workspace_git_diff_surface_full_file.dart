part of 'workspace_git_diff_surface.dart';

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
  if (bytes == null) return null;
  final String text;
  try {
    text = utf8.decode(bytes);
  } on FormatException {
    return null;
  }
  final lines = _splitFullFileLines(text);
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

List<String> _splitFullFileLines(String text) {
  if (text.isEmpty) return const <String>[];
  final normalized = text.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
  final lines = normalized.split('\n');
  if (normalized.endsWith('\n')) lines.removeLast();
  return lines;
}
