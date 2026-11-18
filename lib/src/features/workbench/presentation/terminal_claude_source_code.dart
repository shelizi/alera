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
  final displayed = _logicalLineAt(terminal.buffer, offset.y);
  if (displayed == null || displayed.text.trim().isEmpty) return null;

  TerminalCodeBlock? best;
  var bestScore = 0;
  var ambiguous = false;
  for (final source in sourceBlocks) {
    final codeLines = source.code.split('\n');
    for (var index = 0; index < codeLines.length; index++) {
      final hovered = _matchingCodeLineAt(
        terminal.buffer,
        offset.y,
        codeLines[index],
      );
      if (hovered == null) continue;

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
    // Keep following the actual rendered snippet to its end. Five matching
    // lines are sufficient to verify identity, but not to position a button
    // *after* a 100-line code block when the user hovers near its beginning.
    // Bound the walk to avoid a full scrollback scan for huge files.
    while (sourceIndex >= 0 &&
        sourceIndex < source.length &&
        matched < 600 &&
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
        final witness = _matchingCodeLineAt(
          buffer,
          logical.first,
          source[sourceIndex],
        );
        if (witness != null) {
          first = math.min(first, witness.first);
          last = math.max(last, witness.last);
          terminalRow = direction < 0 ? witness.first - 1 : witness.last + 1;
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

/// A TUI may explicitly split a long Markdown line into several screen rows
/// without marking those rows with xterm automatic wrapping. Reassemble only
/// when their entire text equals one canonical source line, never a prefix.
_LogicalLine? _matchingCodeLineAt(
  xterm.Buffer buffer,
  int row,
  String original,
) {
  final current = _logicalLineAt(buffer, row);
  if (current == null) return null;
  if (_sameCodeLine(current.text, original)) return current;
  if (original.trim().length < buffer.viewWidth - 8) return null;

  final expected = original.trim();
  var start = current.first;
  for (var backwards = 0; backwards < 18 && start >= 0; backwards++) {
    final joined = StringBuffer();
    var nextRow = start;
    var containsHover = false;
    for (
      var forward = 0;
      forward < 18 && nextRow < buffer.lines.length;
      forward++
    ) {
      final part = _logicalLineAt(buffer, nextRow);
      if (part == null) break;
      var display = part.text.trim();
      if (forward == 0) {
        display = display.replaceFirst(RegExp(r'^[•●│┃>][ \t]+'), '');
      }
      joined.write(display);
      if (part.first <= current.first && part.last >= current.last) {
        containsHover = true;
      }
      if (joined.length > expected.length) break;
      if (forward > 0 && containsHover && joined.toString() == expected) {
        return _LogicalLine(start, part.last, joined.toString());
      }
      if (display.isEmpty) break;
      nextRow = part.last + 1;
    }
    if (start == 0) break;
    final previous = _logicalLineAt(buffer, start - 1);
    if (previous == null || previous.text.trim().isEmpty) break;
    start = previous.first;
  }
  return null;
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
