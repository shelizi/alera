import 'dart:math' as math;

import 'package:alera/src/features/agent_status/application/claude_code_message_store.dart';
import 'package:alera/src/features/workbench/presentation/terminal_code_block.dart';
import 'package:xterm2/xterm.dart' as xterm;

/// Match a hovered *rendered* line to a canonical Markdown code block.
///
/// Screen text only locates the block. Clipboard contents always come from
/// the Claude hook. Two source lines must agree for a multi-line block,
/// preventing a prose paragraph or a shared brace from selecting code.
TerminalCodeBlock? resolveClaudeSourceCodeBlockAt({
  required xterm.Terminal terminal,
  required xterm.CellOffset offset,
  required List<ClaudeSourceCodeBlock> sourceBlocks,
}) {
  final lines = terminal.buffer.lines;
  if (offset.y < 0 || offset.y >= lines.length) return null;
  final hovered = _logicalLineAt(terminal.buffer, offset.y);
  if (hovered == null || hovered.text.trim().isEmpty) return null;

  TerminalCodeBlock? best;
  var bestScore = 0;
  var ambiguous = false;
  for (final source in sourceBlocks) {
    final codeLines = source.code.split('\n');
    for (var index = 0; index < codeLines.length; index++) {
      if (!_sameCodeLine(hovered.text, codeLines[index])) continue;

      final evidence = _findWitnesses(
        terminal.buffer,
        hovered,
        codeLines,
        index,
      );
      final score = evidence.score;
      final minScore =
          codeLines.where((line) => line.trim().isNotEmpty).length > 1 ? 2 : 1;
      if (score < minScore || hovered.text.trim().length < 2) continue;
      // A one-line snippet must be substantial enough not to match ordinary
      // punctuation or an incidental Markdown list decoration.
      if (minScore == 1 && codeLines[index].trim().length < 8) continue;

      final candidate = TerminalCodeBlock(
        openingRow: evidence.firstRow,
        closingRow: evidence.lastRow,
        code: source.code,
      );
      if (score > bestScore) {
        best = candidate;
        bestScore = score;
        ambiguous = false;
      } else if (score == bestScore && best?.code != candidate.code) {
        ambiguous = true;
      }
    }
  }
  return ambiguous ? null : best;
}

({int score, int firstRow, int lastRow}) _findWitnesses(
  xterm.Buffer buffer,
  _LogicalLine hovered,
  List<String> source,
  int index,
) {
  var score = 1;
  var first = hovered.first;
  var last = hovered.last;

  for (final direction in const <int>[-1, 1]) {
    var sourceIndex = index + direction;
    var terminalRow = direction < 0 ? hovered.first - 1 : hovered.last + 1;
    var matched = 0;
    while (sourceIndex >= 0 &&
        sourceIndex < source.length &&
        matched < 5 &&
        terminalRow >= 0 &&
        terminalRow < buffer.lines.length) {
      // Markdown preserves empty lines but Ink can add/remove spacers.
      while (sourceIndex >= 0 &&
          sourceIndex < source.length &&
          source[sourceIndex].trim().isEmpty) {
        sourceIndex += direction;
      }
      if (sourceIndex < 0 || sourceIndex >= source.length) break;
      var found = false;
      var skipped = 0;
      while (skipped < 4 &&
          terminalRow >= 0 &&
          terminalRow < buffer.lines.length) {
        final logical = _logicalLineAt(buffer, terminalRow);
        if (logical == null) break;
        terminalRow = direction < 0 ? logical.first - 1 : logical.last + 1;
        skipped++;
        if (_sameCodeLine(logical.text, source[sourceIndex])) {
          first = math.min(first, logical.first);
          last = math.max(last, logical.last);
          score++;
          matched++;
          sourceIndex += direction;
          found = true;
          break;
        }
        // A small number of renderer spacers is harmless, but non-code
        // text between two source lines is not proof of one code block.
        if (logical.text.trim().isNotEmpty) break;
      }
      if (!found) break;
    }
  }
  return (score: score, firstRow: first, lastRow: last);
}

final class _LogicalLine {
  const _LogicalLine(this.first, this.last, this.text);
  final int first;
  final int last;
  final String text;
}

_LogicalLine? _logicalLineAt(xterm.Buffer buffer, int row) {
  final lines = buffer.lines;
  if (row < 0 || row >= lines.length) return null;
  var first = row;
  while (first > 0 && lines[first].isWrapped) {
    first--;
  }
  var last = row;
  while (last + 1 < lines.length && lines[last + 1].isWrapped) {
    last++;
  }
  final joined = StringBuffer();
  for (var line = first; line <= last; line++) {
    joined.write(lines[line].getText());
  }
  return _LogicalLine(first, last, joined.toString());
}

bool _sameCodeLine(String displayed, String original) {
  final code = original.trim();
  final screen = displayed.trim();
  if (code.isEmpty || screen.isEmpty) return false;
  if (screen == code) return true;
  // Claude's terminal renderer may add a bullet or quote gutter to the first
  // source row. This is presentation structure, not language-specific syntax.
  final presentation = RegExp(r'^[•●│┃>][ \t]+');
  return screen.replaceFirst(presentation, '').trim() == code;
}
