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

/// Claude Code may draw its virtualized-scrollback marker on a nearby TUI row,
/// *not inside* the visible code text. Copying only the parsed code in that
/// situation silently truncates a file even though the button appears.
///
/// Inspect a small area around the code block's final row. A marker elsewhere
/// in the terminal scrollback (for example, a different Claude answer) must
/// not force transcript recovery for this block.
bool terminalCodeBlockHasViewportHint({
  required xterm.Terminal terminal,
  required TerminalCodeBlock block,
}) {
  if (block.code.contains('Jump to bottom')) return true;
  final lines = terminal.buffer.lines;
  if (lines.length == 0) return false;
  final first = math.max(0, block.closingRow - 8);
  final last = math.min(lines.length - 1, block.closingRow + 12);
  for (var row = first; row <= last; row++) {
    if (lines[row].getText().contains('Jump to bottom')) return true;
  }
  return false;
}

/// A virtualized Claude response can lose its viewport marker while retaining
/// only the start of a Swift/Objective-C file. A filename-qualified heading and
/// an unfinished source body together are strong evidence that copying the
/// visible fragment alone would lose part of the file.
bool terminalCodeBlockNeedsCanonicalRecovery({
  required xterm.Terminal terminal,
  required TerminalCodeBlock block,
}) {
  if (terminalCodeBlockHasViewportHint(terminal: terminal, block: block)) {
    return true;
  }
  final lines = terminal.buffer.lines;
  if (lines.length == 0 || block.code.split('\n').length < 12) return false;

  var hasSourceFilename = false;
  for (
    var row = math.max(0, block.openingRow - 3);
    row < block.openingRow && row < lines.length;
    row++
  ) {
    if (_isFilenameQualifiedCodeHeading(lines[row].getText())) {
      hasSourceFilename = true;
      break;
    }
  }
  if (!hasSourceFilename) return false;

  // A complete Swift/Obj-C file must close the scopes it opened. On a
  // virtualized screen the first few method bodies are often visible but
  // the final class/struct closing brace has already scrolled away.
  var braceDepth = 0;
  for (final unit in block.code.codeUnits) {
    if (unit == 123) braceDepth++; // {
    if (unit == 125) braceDepth--; // }
  }
  if (braceDepth > 0) return true;

  // Some complete-looking declarations are followed by additional source
  // after Ink's spacer rows. Detect those instead of quietly copying a prefix.
  final peekEnd = math.min(lines.length - 1, block.closingRow + 5);
  for (var row = block.closingRow + 1; row <= peekEnd; row++) {
    final raw = lines[row].getText();
    if (_claudeLanguageHeading.hasMatch(raw) ||
        _claudeTurnBoundary.hasMatch(raw.trimLeft())) {
      break;
    }
    if (raw.trim().isEmpty) continue;
    return !lines[row].isWrapped && _looksLikeLanguageCode(raw);
  }
  return false;
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
  // Multi-language Claude replies can contain hundreds of source lines
  // between their first response bullet and a later Swift section.
  // Scan up to the already bounded caller range, stopping at the turn boundary.
  final first = math.max(0, start - maxScanRows);
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
    // "• Objective-C (queens.m)" is a label, not a function call.
    if (_claudeLanguageHeading.hasMatch(rawOpening) ||
        _claudeLanguageHeading.hasMatch(openingCode) ||
        !_probableCodeOpener.hasMatch(openingCode)) {
      continue;
    }

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
/// An assistant can render a labeled code block as "Swift (queens.swift)"
/// or "• Objective-C (queens.m):" without Markdown backticks. Accept the
/// language and optional filename for any known language, but never a random
/// prose heading or command result.
final RegExp _claudeLanguageHeading = RegExp(
  r'^[ \t]{0,4}(?:[•●][ \t]+)?'
  r'(?:Python(?:\s+\d+(?:\.\d+)*)?|JavaScript|TypeScript|'
  r'Objective-C|Objective C|Obj-C|ObjC|C\+\+|C#|C|Java|'
  r'PowerShell|Bash|Shell|Rust|Go|Dart|PHP|Ruby|Kotlin|'
  r'Swift|SQL|HTML|CSS|JSON|YAML|XML)'
  r'(?:[ \t]*\([A-Za-z0-9_.+/-]{1,80}\))?:?[ \t]*$',
  caseSensitive: false,
);

/// A virtualized Claude TUI may retain only its current screen, dropping the
/// earlier '•' reply marker. A language *with a matching source filename*
/// remains an unambiguous code section even when the introducing bullet was
/// evicted from scrollback. Plain "Swift" prose still requires reply context.
bool _isFilenameQualifiedCodeHeading(String raw) {
  if (!_claudeLanguageHeading.hasMatch(raw)) return false;
  final match = RegExp(r'\(([A-Za-z0-9_.+/-]{1,80})\):?\s*$')
      .firstMatch(raw.trimRight());
  if (match == null) return false;
  final filename = match.group(1)!.toLowerCase();
  return RegExp(
    r'\.(?:swift|m|mm|h|hpp|c|cc|cpp|cs|rs|go|'
    r'dart|py|js|jsx|ts|tsx|java|kt|php|rb|sh|'
    r'ps1|sql|html|css|json|yaml|yml|xml)$',
  ).hasMatch(filename);
}

final RegExp _languageCodeStatement = RegExp(
  r'^(?:public|private|protected|static|void|int|long|float|'
  r'double|bool|boolean|char|String|using|namespace|'
  r'NSInteger|NSUInteger|BOOL|instancetype|id|NS[A-Z]\w*|'
  r'func|mutating|nonmutating|override|required|convenience|'
  r'final|extension|protocol|actor|subscript|init|deinit|'
  r'Write-Output|Write-Host|Get-Content|Set-Content|'
  r'echo|puts|printf|println|System)\b',
  caseSensitive: false,
);

/// Recognize language-neutral source structure plus common Swift/Obj-C
/// declarations. The heading and Claude-turn checks outside this function
/// prevent ordinary terminal output and English paragraphs becoming code.
bool _looksLikeLanguageCode(String text) {
  final source = text.trimLeft();
  if (source.isEmpty || _claudeLanguageHeading.hasMatch(text)) return false;
  if (_probableCodeOpener.hasMatch(source) ||
      _languageCodeStatement.hasMatch(source) ||
      _claudeBareControlLine.hasMatch(source)) {
    return true;
  }
  if (RegExp(
    r'^[@#](?:import|interface|implementation|property|end|'
    r'protocol|class|synthesize|dynamic|autoreleasepool|'
    r'available|objc|MainActor|include|define|pragma)\b',
  ).hasMatch(source))
    return true;
  if (RegExp(r'^[+-]\s*\([^)]*\)\s*[A-Za-z_]\w*').hasMatch(source)) {
    return true;
  }
  if (source.startsWith('//') ||
      source.startsWith('/*') ||
      source.startsWith('*/') ||
      source.startsWith(r'$') ||
      source == '{' ||
      source == '}' ||
      source == '};' ||
      source == ']' ||
      source == ')' ||
      source.endsWith('{') ||
      source.endsWith('}') ||
      source.endsWith('=>')) {
    return true;
  }
  if (_canonicalCodeAssignment.hasMatch(source)) return true;
  if (source.endsWith(';') && RegExp(r'[=();\[\].]').hasMatch(source)) {
    return true;
  }
  // Objective-C sends, Swift property access and method calls.
  if (source.startsWith('[') && source.endsWith(';')) return true;
  return false;
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
    final headingLine = lines[header].getText();
    if (!_claudeCodeBullet.hasMatch(headingLine) &&
        !_isFilenameQualifiedCodeHeading(headingLine) &&
        !_insideClaudeResponse(buffer, header, maxScanRows: maxScanRows)) {
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
        // Claude's virtualized Ink renderer sometimes inserts multiple
        // spacer rows inside a source block (especially after Swift braces).
        // Look ahead across a few blanks, but never cross another title or
        // into ordinary explanation text.
        var nextRow = row + 1;
        final peekEnd = math.min(row + 4, lines.length - 1);
        while (nextRow <= peekEnd &&
            !lines[nextRow].isWrapped &&
            lines[nextRow].getText().trim().isEmpty) {
          nextRow++;
        }
        if (nextRow <= peekEnd &&
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
      // A code presentation may add two leading gutter cells to every row.
      // Strip only that shared offset, keeping nested language indentation.
      final indents = source
          .where((line) => line.isNotEmpty)
          .map((line) => line.length - line.trimLeft().length);
      final gutter = indents.isEmpty ? 0 : indents.reduce(math.min);
      return TerminalCodeBlock(
        openingRow: codeStart,
        closingRow: lastRow,
        code: source
            .map(
              (line) =>
                  line.isEmpty || gutter == 0 ? line : line.substring(gutter),
            )
            .join('\n'),
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
  r'^(?:#(?:include|import|define|pragma|ifn?def|endif)\b|'
  r'@(?:interface|implementation|protocol|import|class|MainActor)\b|'
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

/// Return a whole source block, including blank lines and nested indentation.
/// The first row may include Claude's leading '• ' bullet; the other rows
/// retain the presentation gutter and are dedented independently.
TerminalCodeBlock? _collectClaudeBareCode(
  xterm.Buffer buffer,
  int start, {
  required int maxScanRows,
  String? bulletCode,
}) {
  final lines = buffer.lines;
  final source = <String>[];
  var lastRow = start - 1;
  for (
    var row = start;
    row < lines.length && row <= start + maxScanRows;
    row++
  ) {
    final line = lines[row];
    final raw = row == start && bulletCode != null
        ? bulletCode
        : line.getText();
    if (line.isWrapped) {
      if (source.isEmpty) break;
      final wrapsAgain = row + 1 < lines.length && lines[row + 1].isWrapped;
      source[source.length - 1] += wrapsAgain ? raw : raw.trimRight();
      lastRow = row;
      continue;
    }
    if (row > start &&
        (_claudeTurnBoundary.hasMatch(raw.trimLeft()) ||
            _claudeCodeBullet.hasMatch(raw) ||
            _claudeLanguageHeading.hasMatch(raw))) {
      break;
    }
    if (raw.trim().isEmpty) {
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
  if (source.isEmpty) return null;

  // Strip only Claude's common presentation gutter. When the first line
  // has a bullet, exclude that line from the indentation calculation:
  // '#include' in '• #include' is already flush-left source text.
  final unbulleted = bulletCode == null ? source : source.skip(1).toList();
  final indents = unbulleted
      .where((line) => line.isNotEmpty)
      .map((line) => line.length - line.trimLeft().length);
  final gutter = indents.isEmpty ? 0 : indents.reduce(math.min);
  final codeLines = <String>[];
  for (var i = 0; i < source.length; i++) {
    final line = source[i];
    if (line.isEmpty || gutter == 0 || (bulletCode != null && i == 0)) {
      codeLines.add(line);
    } else {
      codeLines.add(line.substring(gutter));
    }
  }
  return TerminalCodeBlock(
    openingRow: start,
    closingRow: lastRow,
    code: codeLines.join('\n'),
  );
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

  // A Claude reply may begin with code itself ('• #include <stdio.h>'),
  // followed by unbulleted lines. That is one copyable source block.
  final bulletCode = _claudeCodeBullet
      .firstMatch(lines[replyBullet].getText())
      ?.group(1);
  if (bulletCode != null &&
      _claudeBareCodeStart.hasMatch(bulletCode.trimLeft())) {
    final block = _collectClaudeBareCode(
      buffer,
      replyBullet,
      maxScanRows: maxScanRows,
      bulletCode: bulletCode,
    );
    if (block?.containsRow(hoveredRow) == true) return block;
  } else if (bulletCode != null && _probableCodeOpener.hasMatch(bulletCode)) {
    // Keep short commands handled by the existing bullet-specific decoder.
    return null;
  }

  // For prose-introduced source, use its first real declaration instead of
  // a later nested function statement under the hovered row.
  for (var start = replyBullet + 1; start <= hoveredRow; start++) {
    if (lines[start].isWrapped ||
        !_claudeBareCodeStart.hasMatch(lines[start].getText().trimLeft())) {
      continue;
    }
    final block = _collectClaudeBareCode(
      buffer,
      start,
      maxScanRows: maxScanRows,
    );
    if (block?.containsRow(hoveredRow) == true) return block;
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

/// Recovers a complete Claude source snippet from a canonical assistant message
/// when the terminal TUI has virtualized/truncated its on-screen scrollback.
///
/// Only accept a response whose first four nonempty source lines match the
/// visible copy block. It must contain more code than the terminal currently
/// holds, preventing an older or unrelated Claude answer from replacing it.
/// The canonical JSONL contains assistant prose after the source. Unlike the
/// terminal's relaxed hover heuristic, this stricter extractor must never
/// accept an English paragraph merely because it mentions "N = 8".
final RegExp _canonicalCodeAssignment = RegExp(
  r'^[A-Za-z_][\w.\[\]]*\s*(?:=|\+=|-=|\*=|/=)\s*\S',
);

bool _looksLikeCanonicalClaudeCodeLine(String raw) {
  final source = raw.trimLeft();
  if (source.isEmpty) return false;
  if (_claudeBareCodeStart.hasMatch(source) || _looksLikeLanguageCode(source)) {
    return true;
  }
  if (source.startsWith('#') ||
      source.startsWith('//') ||
      source.startsWith('/*') ||
      source.startsWith('*/') ||
      source.startsWith('* ')) {
    return true;
  }
  if (source == '{' ||
      source == '}' ||
      source == '};' ||
      source == ']' ||
      source == ')') {
    return true;
  }
  return source.endsWith(';') ||
      source.endsWith('{') ||
      source.endsWith('}') ||
      source.endsWith('=>') ||
      _canonicalCodeAssignment.hasMatch(source);
}

String? recoverCompleteClaudeSourceFromText({
  required String visibleCode,
  required String assistantText,
}) {
  final visible = visibleCode.replaceAll('\r\n', '\n').split('\n');
  final signature = visible
      .map((line) => line.trim())
      .where((line) => line.isNotEmpty)
      .take(4)
      .toList();
  if (signature.length < 4 || visible.length < 12) return null;

  final lines = assistantText.replaceAll('\r\n', '\n').split('\n');
  final visibleLineCount = visible.length;
  for (var start = 0; start < lines.length; start++) {
    if (lines[start].trim() != signature.first ||
        !_claudeBareCodeStart.hasMatch(lines[start].trimLeft())) {
      continue;
    }
    final codeLines = <String>[];
    for (var i = start; i < lines.length; i++) {
      final line = lines[i].trimRight();
      if (line.isEmpty) {
        if (i + 1 < lines.length &&
            _looksLikeCanonicalClaudeCodeLine(lines[i + 1])) {
          codeLines.add('');
          continue;
        }
        break;
      }
      if (!_looksLikeCanonicalClaudeCodeLine(line)) break;
      codeLines.add(line);
    }
    final canonicalSignature = codeLines
        .map((line) => line.trim())
        .where((line) => line.isNotEmpty)
        .take(4)
        .toList();
    if (canonicalSignature.length != signature.length) continue;
    var matches = true;
    for (var i = 0; i < signature.length; i++) {
      if (canonicalSignature[i] != signature[i]) {
        matches = false;
        break;
      }
    }
    if (!matches || codeLines.length <= visibleLineCount) continue;
    return codeLines.join('\n');
  }
  return null;
}
