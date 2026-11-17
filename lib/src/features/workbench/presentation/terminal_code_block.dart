import 'dart:math' as math;

import 'package:xterm2/xterm.dart' as xterm;

/// A fenced code block in rendered terminal scrollback. Delimiters are omitted
/// from [code] so copying does not paste Markdown fences.
final class TerminalCodeBlock {
  const TerminalCodeBlock({
    required this.openingRow,
    required this.closingRow,
    required this.code,
  });

  final int openingRow;
  final int closingRow;
  final String code;

  bool containsRow(int row) => row >= openingRow && row <= closingRow;
}

/// Styling alone cannot safely distinguish code from ordinary terminal output.
/// Scan a bounded range only when the mouse crosses into a different row.
TerminalCodeBlock? resolveTerminalCodeBlockAt({
  required xterm.Terminal terminal,
  required xterm.CellOffset offset,
  int maxScanRows = 600,
}) {
  final lines = terminal.buffer.lines;
  if (offset.y < 0 || offset.y >= lines.length || maxScanRows < 1) {
    return null;
  }

  final first = math.max(0, offset.y - maxScanRows);
  final last = math.min(lines.length - 1, offset.y + maxScanRows);
  _CodeFence? opening;
  for (var row = first; row <= last; row++) {
    // A wrapped source line must never begin or end a Markdown fence.
    if (lines[row].isWrapped) continue;
    final fence = _CodeFence.parse(lines[row].getText(), row);
    if (fence == null) continue;
    if (opening == null) {
      opening = fence;
      continue;
    }

    if (fence.isClosingFor(opening)) {
      if (offset.y >= opening.row && offset.y <= row && row > opening.row + 1) {
        final code = _collectCode(terminal.buffer, opening, row);
        if (code.trim().isNotEmpty) {
          return TerminalCodeBlock(
            openingRow: opening.row,
            closingRow: row,
            code: code,
          );
        }
      }
      opening = null;
    } else if (fence.language.isNotEmpty) {
      // A language-labelled opener cannot close a preceding fence. This
      // avoids joining unrelated blocks if scanning begins mid-scrollback.
      opening = fence;
    }
  }
  return _resolveClaudeRenderedCodeAt(
    terminal.buffer,
    offset.y,
    maxScanRows: maxScanRows,
  );
}

final RegExp _claudeCodeBullet = RegExp(r'^[ \t]{0,3}[•●] (.+)$');

/// Claude Code's Ink renderer may output the code without backticks. The
/// leading response bullet is followed by code lines indented two cells.
/// Only enable the copy overlay when the first line looks like source code.
final RegExp _probableCodeOpener = RegExp(
  r'^(?:(?:const|let|var|export|import|function|class|interface|type|'
  r'async|await|def|print|from|return|fn|pub|use|struct|impl|enum|'
  r'package|#include|SELECT|INSERT|UPDATE|CREATE|WITH)\b|'
  r'\{|<|\[|(?:[\w.-]+\s*\()|(?:[\w.-]+\s*[:=]\s*)|'
  r'(?:\$\s*)?(?:git|npm|pnpm|yarn|cargo|curl|docker|'
  r'kubectl|python|node|flutter|ls|cd)\s+)',
  caseSensitive: false,
);

TerminalCodeBlock? _resolveClaudeRenderedCodeAt(
  xterm.Buffer buffer,
  int hoveredRow, {
  required int maxScanRows,
}) {
  final lines = buffer.lines;
  final first = math.max(0, hoveredRow - math.min(maxScanRows, 150));
  for (var start = hoveredRow; start >= first; start--) {
    if (lines[start].isWrapped) continue;
    final match = _claudeCodeBullet.firstMatch(lines[start].getText());
    if (match == null) continue;
    final openingCode = match.group(1)!.trimRight();
    if (!_probableCodeOpener.hasMatch(openingCode)) continue;

    final parts = <String>[openingCode];
    var lastRow = start;
    for (
      var row = start + 1;
      row < lines.length && row <= start + math.min(maxScanRows, 150);
      row++
    ) {
      final line = lines[row];
      final raw = line.getText();
      if (line.isWrapped) {
        parts[parts.length - 1] += raw.trimRight();
        lastRow = row;
        continue;
      }
      if (raw.trim().isEmpty) {
        // Keep a blank source line only if source indentation resumes.
        if (row + 1 < lines.length &&
            !lines[row + 1].isWrapped &&
            lines[row + 1].getText().startsWith('  ') &&
            lines[row + 1].getText().trim().isNotEmpty) {
          parts.add('');
          lastRow = row;
          continue;
        }
        break;
      }
      if (!raw.startsWith('  ')) break;
      parts.add(raw.substring(2).trimRight());
      lastRow = row;
    }

    if (hoveredRow <= lastRow) {
      return TerminalCodeBlock(
        openingRow: start,
        closingRow: lastRow,
        code: parts.join('\n'),
      );
    }
  }
  return null;
}

String _collectCode(xterm.Buffer buffer, _CodeFence opening, int closingRow) {
  final lines = buffer.lines;
  final code = StringBuffer();
  for (var row = opening.row + 1; row < closingRow; row++) {
    final line = lines[row];
    final wrapped = line.isWrapped;
    if (row > opening.row + 1 && !wrapped) code.write('\n');
    var text = line.getText();
    // Claude-style quote gutters are presentation, not source code.
    if (!wrapped &&
        opening.prefix.isNotEmpty &&
        text.startsWith(opening.prefix)) {
      text = text.substring(opening.prefix.length);
    }
    // A wrapped row's trailing cell may be part of a single source line.
    final continuesOnNextRow = row + 1 < closingRow && lines[row + 1].isWrapped;
    code.write(continuesOnNextRow ? text : text.trimRight());
  }
  return code.toString();
}

final class _CodeFence {
  const _CodeFence({
    required this.row,
    required this.prefix,
    required this.marker,
    required this.language,
  });

  final int row;
  final String prefix;
  final String marker;
  final String language;

  bool isClosingFor(_CodeFence opening) =>
      marker.codeUnitAt(0) == opening.marker.codeUnitAt(0) &&
      marker.length >= opening.marker.length &&
      language.isEmpty;

  static final RegExp _pattern = RegExp(
    r'^([ \t]{0,4}(?:(?:│|┃|>) ?)?)(`{3,}|~{3,})(.*)$',
  );

  static _CodeFence? parse(String text, int row) {
    final match = _pattern.firstMatch(text.trimRight());
    if (match == null) return null;
    return _CodeFence(
      row: row,
      prefix: match.group(1)!,
      marker: match.group(2)!,
      language: match.group(3)!.trim(),
    );
  }
}
