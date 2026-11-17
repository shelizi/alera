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
  return _resolveClaudeLanguageCodeAt(
        terminal.buffer,
        offset.y,
        maxScanRows: maxScanRows,
      ) ??
      _resolveClaudeBareCodeAt(
        terminal.buffer,
        offset.y,
        maxScanRows: maxScanRows,
      ) ??
      _resolveClaudeRenderedCodeAt(
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

final RegExp _claudeTurnBoundary = RegExp(
  r'^(?:❯(?:\s|$)|[✻✦✶]\s+|[*]\s+(?:Cooked|Brewed|Crunched|Done)\b)',
);

/// Unbulleted snippets can follow Claude's prose or earlier code blocks.
/// Only consider them when an actual response bullet appears in this turn.
bool _insideClaudeResponse(
  xterm.Buffer buffer,
  int start, {
  required int maxScanRows,
}) {
  final lines = buffer.lines;
  final first = math.max(0, start - math.min(maxScanRows, 150));
  for (var row = start - 1; row >= first; row--) {
    if (lines[row].isWrapped) continue;
    final text = lines[row].getText();
    if (_claudeTurnBoundary.hasMatch(text.trimLeft())) return false;
    if (_claudeCodeBullet.hasMatch(text)) return true;
  }
  return false;
}

/// A Claude response can indent both source and ordinary explanation.
/// A blank line may stay inside a copied snippet only when the next
/// indented line still looks like source, not a prose paragraph.
bool _looksLikeIndentedCodeAfterBlank(String raw) {
  if (!raw.startsWith('  ')) return false;
  final source = raw.substring(2).trimLeft();
  if (source.isEmpty) return false;
  return _probableCodeOpener.hasMatch(source) ||
      source.startsWith('}') ||
      source.startsWith(']') ||
      source.startsWith(')') ||
      source.startsWith('//') ||
      source.startsWith('/*') ||
      source.startsWith('*/');
}

TerminalCodeBlock? _resolveClaudeRenderedCodeAt(
  xterm.Buffer buffer,
  int hoveredRow, {
  required int maxScanRows,
}) {
  final lines = buffer.lines;
  final scanLimit = math.min(maxScanRows, 150);
  final first = math.max(0, hoveredRow - scanLimit);
  for (var start = hoveredRow; start >= first; start--) {
    if (lines[start].isWrapped) continue;
    final rawOpening = lines[start].getText();
    final bullet = _claudeCodeBullet.firstMatch(rawOpening);
    final unbulleted = bullet == null && rawOpening.startsWith('  ');
    if (bullet == null && !unbulleted) continue;

    if (unbulleted) {
      // The second physical row of an existing snippet is continuation,
      // not another copyable block. A new block starts after prose or a gap.
      if (start > 0) {
        final previous = lines[start - 1];
        final previousText = previous.getText();
        if (previous.isWrapped ||
            previousText.startsWith('  ') ||
            _claudeCodeBullet.hasMatch(previousText)) {
          continue;
        }
      }
      if (!_insideClaudeResponse(buffer, start, maxScanRows: maxScanRows)) {
        continue;
      }
    }

    // A wrapped opener such as "• console.log(42)" might be split before
    // its opening parenthesis. Match the logical line, not its first row.
    var openingCode = bullet?.group(1) ?? rawOpening.substring(2);
    var openingLastRow = start;
    while (openingLastRow + 1 < lines.length &&
        lines[openingLastRow + 1].isWrapped) {
      openingLastRow++;
      openingCode += lines[openingLastRow].getText();
    }
    openingCode = openingCode.trimRight();
    if (!_probableCodeOpener.hasMatch(openingCode)) continue;

    final parts = <String>[openingCode];
    var lastRow = openingLastRow;
    for (
      var row = openingLastRow + 1;
      row < lines.length && row <= start + scanLimit;
      row++
    ) {
      final line = lines[row];
      final raw = line.getText();
      if (line.isWrapped) {
        final continues = row + 1 < lines.length && lines[row + 1].isWrapped;
        parts[parts.length - 1] += continues ? raw : raw.trimRight();
        lastRow = row;
        continue;
      }
      if (raw.trim().isEmpty) {
        if (row + 1 < lines.length &&
            !lines[row + 1].isWrapped &&
            _looksLikeIndentedCodeAfterBlank(lines[row + 1].getText())) {
          parts.add('');
          lastRow = row;
          continue;
        }
        break;
      }
      if (!raw.startsWith('  ')) break;
      final continues = row + 1 < lines.length && lines[row + 1].isWrapped;
      final source = raw.substring(2);
      parts.add(continues ? source : source.trimRight());
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

/// Claude Code also renders syntax-highlighted snippets with plain language
/// headings instead of backtick fences or a bullet on the first source line.
/// Only detect those headings inside a Claude reply, never general PTY output.
final RegExp _claudeLanguageHeading = RegExp(
  r'^[ \t]{0,4}(?:Python(?:\s+\d+(?:\.\d+)*)?|'
  r'JavaScript(?:\s*\((?:Node(?:\.js)?|Deno|Bun)\))?|'
  r'TypeScript|C\+\+|C#|C|Java|PowerShell|Bash|Shell|'
  r'Rust|Go|Dart|PHP|Ruby|Kotlin|Swift|SQL|HTML|CSS|JSON|YAML|XML)'
  r':?[ \t]*$',
  caseSensitive: false,
);

final RegExp _languageCodeStatement = RegExp(
  r'^(?:public|private|protected|static|void|int|long|float|'
  r'double|bool|boolean|char|String|using|namespace|'
  r'Write-Output|Write-Host|Get-Content|Set-Content|'
  r'echo|puts|printf|println|System)\b',
  caseSensitive: false,
);

bool _looksLikeLanguageCode(String text) {
  final source = text.trimLeft();
  if (source.isEmpty) return false;
  return _probableCodeOpener.hasMatch(source) ||
      _languageCodeStatement.hasMatch(source) ||
      source.startsWith(r'$') ||
      source.startsWith('#include') ||
      source.startsWith('//') ||
      source.startsWith('/*') ||
      source.startsWith('*/') ||
      source.startsWith('}') ||
      source.startsWith(']') ||
      source.startsWith(')');
}

TerminalCodeBlock? _resolveClaudeLanguageCodeAt(
  xterm.Buffer buffer,
  int hoveredRow, {
  required int maxScanRows,
}) {
  final lines = buffer.lines;
  final first = math.max(0, hoveredRow - math.min(maxScanRows, 150));
  for (var header = hoveredRow - 1; header >= first; header--) {
    if (lines[header].isWrapped ||
        !_claudeLanguageHeading.hasMatch(lines[header].getText())) {
      continue;
    }
    if (!_insideClaudeResponse(buffer, header, maxScanRows: maxScanRows)) {
      continue;
    }

    var codeStart = header + 1;
    // Allow one presentation spacer between a label and its source.
    if (codeStart < lines.length && lines[codeStart].getText().trim().isEmpty) {
      codeStart++;
    }
    if (codeStart >= lines.length ||
        lines[codeStart].isWrapped ||
        !_looksLikeLanguageCode(lines[codeStart].getText())) {
      continue;
    }

    final source = <String>[];
    var lastRow = codeStart - 1;
    for (
      var row = codeStart;
      row < lines.length && row <= codeStart + math.min(maxScanRows, 150);
      row++
    ) {
      final raw = lines[row].getText();
      if (lines[row].isWrapped) {
        if (source.isEmpty) break;
        final wrapsAgain = row + 1 < lines.length && lines[row + 1].isWrapped;
        source[source.length - 1] += wrapsAgain ? raw : raw.trimRight();
        lastRow = row;
        continue;
      }
      if (_claudeLanguageHeading.hasMatch(raw) ||
          _claudeTurnBoundary.hasMatch(raw.trimLeft())) {
        break;
      }
      if (raw.trim().isEmpty) {
        // Preserve blank source lines (e.g. between a C include and main)
        // but stop at a blank before another language or ordinary prose.
        final nextRow = row + 1;
        if (nextRow < lines.length &&
            !lines[nextRow].isWrapped &&
            _looksLikeLanguageCode(lines[nextRow].getText()) &&
            !_claudeLanguageHeading.hasMatch(lines[nextRow].getText())) {
          source.add('');
          lastRow = row;
          continue;
        }
        break;
      }
      if (!_looksLikeLanguageCode(raw)) break;
      final wrapsNext = row + 1 < lines.length && lines[row + 1].isWrapped;
      source.add(wrapsNext ? raw : raw.trimRight());
      lastRow = row;
    }
    if (hoveredRow >= codeStart && hoveredRow <= lastRow) {
      return TerminalCodeBlock(
        openingRow: codeStart,
        closingRow: lastRow,
        code: source.join('\n'),
      );
    }
  }
  return null;
}

/// Claude often emits an entire syntax-highlighted source file without
/// Markdown fences or a "C" title. Detect a strong source opener within a
/// response bullet and continue through declarations, functions and blank
/// lines. Never treat generic PTY output as source by itself.
final RegExp _claudeBareCodeStart = RegExp(
  r'^(?:#(?:include|define|pragma|ifn?def|endif)\b|'
  r'(?:static\s+)?(?:unsigned\s+|signed\s+)?'
  r'(?:void|int|long|short|char|float|double|bool|size_t)\s+\**[A-Za-z_]\w*|'
  r'(?:const|let|var|function|class|interface|type|def|from|import|'
  r'export|fn|pub|struct|impl|enum|package|using|namespace|typedef)\b|'
  r'(?:async\s+)?function\s+|'
  r'(?:print|printf|console\.log|Write-Output)\s*\()',
  caseSensitive: false,
);

final RegExp _claudeBareControlLine = RegExp(
  r'^(?:if|else|for|while|do|switch|case|default|'
  r'try|catch|finally|return|continue|break|throw|'
  r'await|yield|pass|assert)\b',
  caseSensitive: false,
);

bool _looksLikeBareCodeLine(String raw) {
  final source = raw.trimLeft();
  if (source.isEmpty) return false;
  if (_claudeBareCodeStart.hasMatch(source) ||
      _claudeBareControlLine.hasMatch(source) ||
      _looksLikeLanguageCode(source)) {
    return true;
  }
  if (source.startsWith('/*') ||
      source.startsWith('*/') ||
      source.startsWith('//') ||
      source.startsWith('* ') ||
      source.startsWith('#')) {
    return true;
  }
  if (source == '{' || source == '}' || source == '};') return true;
  // Covers ordinary assignment, function calls and array expressions,
  // including lines nested many spaces inside C/JS/Python functions.
  return source.endsWith(';') ||
      source.endsWith('{') ||
      source.endsWith('}') ||
      source.endsWith('=>') ||
      source.endsWith(',') ||
      source.contains(' = ') ||
      source.contains(' += ') ||
      source.contains(' -= ') ||
      source.contains('++') ||
      source.contains('--');
}

TerminalCodeBlock? _resolveClaudeBareCodeAt(
  xterm.Buffer buffer,
  int hoveredRow, {
  required int maxScanRows,
}) {
  final lines = buffer.lines;
  final first = math.max(0, hoveredRow - maxScanRows);
  int? replyBullet;
  for (var row = hoveredRow; row >= first; row--) {
    if (lines[row].isWrapped) continue;
    final raw = lines[row].getText();
    if (_claudeTurnBoundary.hasMatch(raw.trimLeft())) return null;
    if (_claudeCodeBullet.hasMatch(raw)) {
      replyBullet = row;
      break;
    }
  }
  if (replyBullet == null) return null;
  // A source-code bullet already has a dedicated renderer-aware decoder
  // which removes its presentation prefix. Do not turn its indented child
  // rows into a separate bare code block.
  final bulletText = _claudeCodeBullet
      .firstMatch(lines[replyBullet].getText())
      ?.group(1);
  if (bulletText != null && _probableCodeOpener.hasMatch(bulletText)) {
    return null;
  }

  // Use the earliest genuine source opener in the current Claude answer,
  // not the nearest nested "static"/"int" declaration to the hovered row.
  for (var start = replyBullet + 1; start <= hoveredRow; start++) {
    if (lines[start].isWrapped ||
        !_claudeBareCodeStart.hasMatch(lines[start].getText().trimLeft())) {
      continue;
    }

    final source = <String>[];
    var lastRow = start - 1;
    for (
      var row = start;
      row < lines.length && row <= start + maxScanRows;
      row++
    ) {
      final line = lines[row];
      final raw = line.getText();
      if (line.isWrapped) {
        if (source.isEmpty) break;
        final wrapsAgain = row + 1 < lines.length && lines[row + 1].isWrapped;
        source[source.length - 1] += wrapsAgain ? raw : raw.trimRight();
        lastRow = row;
        continue;
      }
      if (_claudeTurnBoundary.hasMatch(raw.trimLeft()) ||
          _claudeCodeBullet.hasMatch(raw) ||
          _claudeLanguageHeading.hasMatch(raw)) {
        break;
      }
      if (raw.trim().isEmpty) {
        // Keep blank lines between preprocessor directives, declarations,
        // functions and comments, but stop before ordinary explanation.
        final nextRow = row + 1;
        if (nextRow < lines.length &&
            !lines[nextRow].isWrapped &&
            _looksLikeBareCodeLine(lines[nextRow].getText())) {
          source.add('');
          lastRow = row;
          continue;
        }
        break;
      }
      if (!_looksLikeBareCodeLine(raw)) break;
      final wrapsNext = row + 1 < lines.length && lines[row + 1].isWrapped;
      source.add(wrapsNext ? raw : raw.trimRight());
      lastRow = row;
    }

    if (hoveredRow <= lastRow && source.isNotEmpty) {
      // Claude's rendered code can have a shared presentation gutter.
      // Strip only that common gutter; preserve nested source indentation.
      final indents = source
          .where((text) => text.isNotEmpty)
          .map((text) => text.length - text.trimLeft().length);
      final gutter = indents.isEmpty ? 0 : indents.reduce(math.min);
      final code = source
          .map((text) {
            if (text.isEmpty || gutter == 0) return text;
            return text.substring(gutter);
          })
          .join('\n');
      return TerminalCodeBlock(
        openingRow: start,
        closingRow: lastRow,
        code: code,
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
