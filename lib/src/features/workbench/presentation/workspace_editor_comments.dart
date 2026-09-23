part of 'workspace_editor_surface.dart';

@visibleForTesting
final class WorkspaceEditorCommentSyntax {
  const WorkspaceEditorCommentSyntax.line(this.linePrefix)
    : blockStart = null,
      blockEnd = null;

  const WorkspaceEditorCommentSyntax.block(this.blockStart, this.blockEnd)
    : linePrefix = null;

  final String? linePrefix;
  final String? blockStart;
  final String? blockEnd;
}

@visibleForTesting
final class WorkspaceEditorCommentEdit {
  const WorkspaceEditorCommentEdit({
    required this.start,
    required this.end,
    required this.replacement,
    required this.selection,
  });

  final int start;
  final int end;
  final String replacement;
  final TextSelection selection;
}

final class _WorkspaceEditorCommentOffsetChange {
  const _WorkspaceEditorCommentOffsetChange({
    required this.start,
    required this.removedLength,
    required this.insertedLength,
    this.shiftAtStart = false,
  });

  final int start;
  final int removedLength;
  final int insertedLength;
  final bool shiftAtStart;
}

extension _WorkspaceEditorComments on _WorkspaceEditorSurfaceState {
  void _toggleEditorComment({
    required String filePath,
    required String languageId,
  }) {
    final syntax = workspaceEditorCommentSyntaxForPath(
      filePath: filePath,
      languageId: languageId,
    );
    if (syntax == null) return;

    final edit = workspaceEditorToggleCommentEdit(
      text: _controller.text,
      selection: _controller.selection,
      syntax: syntax,
    );
    if (edit == null) return;

    _controller.replaceRange(edit.start, edit.end, edit.replacement);
    _controller.setSelectionSilently(edit.selection);
  }
}

@visibleForTesting
WorkspaceEditorCommentSyntax? workspaceEditorCommentSyntaxForPath({
  required String filePath,
  required String languageId,
}) {
  final extension = p.extension(filePath).toLowerCase();
  if (extension == '.toml') {
    return const WorkspaceEditorCommentSyntax.line('#');
  }

  final normalized = languageId.trim().toLowerCase();
  return _workspaceEditorCommentSyntaxes[normalized];
}

const Map<String, WorkspaceEditorCommentSyntax>
_workspaceEditorCommentSyntaxes = <String, WorkspaceEditorCommentSyntax>{
  '1c': WorkspaceEditorCommentSyntax.line('//'),
  'abnf': WorkspaceEditorCommentSyntax.line(';'),
  'ada': WorkspaceEditorCommentSyntax.line('--'),
  'apache': WorkspaceEditorCommentSyntax.line('#'),
  'applescript': WorkspaceEditorCommentSyntax.line('--'),
  'arduino': WorkspaceEditorCommentSyntax.line('//'),
  'actionscript': WorkspaceEditorCommentSyntax.line('//'),
  'asciidoc': WorkspaceEditorCommentSyntax.line('//'),
  'autohotkey': WorkspaceEditorCommentSyntax.line(';'),
  'autoit': WorkspaceEditorCommentSyntax.line(';'),
  'awk': WorkspaceEditorCommentSyntax.line('#'),
  'bash': WorkspaceEditorCommentSyntax.line('#'),
  'shell': WorkspaceEditorCommentSyntax.line('#'),
  'sh': WorkspaceEditorCommentSyntax.line('#'),
  'dos': WorkspaceEditorCommentSyntax.line('REM'),
  'batch': WorkspaceEditorCommentSyntax.line('REM'),
  'c': WorkspaceEditorCommentSyntax.line('//'),
  'cpp': WorkspaceEditorCommentSyntax.line('//'),
  'csharp': WorkspaceEditorCommentSyntax.line('//'),
  'clojure': WorkspaceEditorCommentSyntax.line(';'),
  'cmake': WorkspaceEditorCommentSyntax.line('#'),
  'coffeescript': WorkspaceEditorCommentSyntax.line('#'),
  'crystal': WorkspaceEditorCommentSyntax.line('#'),
  'd': WorkspaceEditorCommentSyntax.line('//'),
  'dart': WorkspaceEditorCommentSyntax.line('//'),
  'dockerfile': WorkspaceEditorCommentSyntax.line('#'),
  'elixir': WorkspaceEditorCommentSyntax.line('#'),
  'elm': WorkspaceEditorCommentSyntax.line('--'),
  'erlang': WorkspaceEditorCommentSyntax.line('%'),
  'fortran': WorkspaceEditorCommentSyntax.line('!'),
  'fsharp': WorkspaceEditorCommentSyntax.line('//'),
  'gcode': WorkspaceEditorCommentSyntax.line(';'),
  'gherkin': WorkspaceEditorCommentSyntax.line('#'),
  'glsl': WorkspaceEditorCommentSyntax.line('//'),
  'go': WorkspaceEditorCommentSyntax.line('//'),
  'graphql': WorkspaceEditorCommentSyntax.line('#'),
  'gradle': WorkspaceEditorCommentSyntax.line('//'),
  'groovy': WorkspaceEditorCommentSyntax.line('//'),
  'haskell': WorkspaceEditorCommentSyntax.line('--'),
  'haxe': WorkspaceEditorCommentSyntax.line('//'),
  'http': WorkspaceEditorCommentSyntax.line('#'),
  'ini': WorkspaceEditorCommentSyntax.line(';'),
  'java': WorkspaceEditorCommentSyntax.line('//'),
  'javascript': WorkspaceEditorCommentSyntax.line('//'),
  'js': WorkspaceEditorCommentSyntax.line('//'),
  'jsx': WorkspaceEditorCommentSyntax.line('//'),
  'json': WorkspaceEditorCommentSyntax.line('//'),
  'jsonc': WorkspaceEditorCommentSyntax.line('//'),
  'julia': WorkspaceEditorCommentSyntax.line('#'),
  'kotlin': WorkspaceEditorCommentSyntax.line('//'),
  'latex': WorkspaceEditorCommentSyntax.line('%'),
  'less': WorkspaceEditorCommentSyntax.line('//'),
  'lisp': WorkspaceEditorCommentSyntax.line(';'),
  'lua': WorkspaceEditorCommentSyntax.line('--'),
  'makefile': WorkspaceEditorCommentSyntax.line('#'),
  'matlab': WorkspaceEditorCommentSyntax.line('%'),
  'nginx': WorkspaceEditorCommentSyntax.line('#'),
  'nim': WorkspaceEditorCommentSyntax.line('#'),
  'nix': WorkspaceEditorCommentSyntax.line('#'),
  'objectivec': WorkspaceEditorCommentSyntax.line('//'),
  'objective-c': WorkspaceEditorCommentSyntax.line('//'),
  'perl': WorkspaceEditorCommentSyntax.line('#'),
  'php': WorkspaceEditorCommentSyntax.line('//'),
  'powershell': WorkspaceEditorCommentSyntax.line('#'),
  'properties': WorkspaceEditorCommentSyntax.line('#'),
  'protobuf': WorkspaceEditorCommentSyntax.line('//'),
  'python': WorkspaceEditorCommentSyntax.line('#'),
  'r': WorkspaceEditorCommentSyntax.line('#'),
  'ruby': WorkspaceEditorCommentSyntax.line('#'),
  'rust': WorkspaceEditorCommentSyntax.line('//'),
  'scala': WorkspaceEditorCommentSyntax.line('//'),
  'scheme': WorkspaceEditorCommentSyntax.line(';'),
  'scss': WorkspaceEditorCommentSyntax.line('//'),
  'sass': WorkspaceEditorCommentSyntax.line('//'),
  'sql': WorkspaceEditorCommentSyntax.line('--'),
  'swift': WorkspaceEditorCommentSyntax.line('//'),
  'typescript': WorkspaceEditorCommentSyntax.line('//'),
  'ts': WorkspaceEditorCommentSyntax.line('//'),
  'tsx': WorkspaceEditorCommentSyntax.line('//'),
  'vbnet': WorkspaceEditorCommentSyntax.line("'"),
  'vbscript': WorkspaceEditorCommentSyntax.line("'"),
  'verilog': WorkspaceEditorCommentSyntax.line('//'),
  'vhdl': WorkspaceEditorCommentSyntax.line('--'),
  'vim': WorkspaceEditorCommentSyntax.line('"'),
  'x86asm': WorkspaceEditorCommentSyntax.line(';'),
  'asm': WorkspaceEditorCommentSyntax.line(';'),
  'yaml': WorkspaceEditorCommentSyntax.line('#'),
  'toml': WorkspaceEditorCommentSyntax.line('#'),
  'css': WorkspaceEditorCommentSyntax.block('/*', '*/'),
  'erb': WorkspaceEditorCommentSyntax.block('<%#', '%>'),
  'html': WorkspaceEditorCommentSyntax.block('<!--', '-->'),
  'markdown': WorkspaceEditorCommentSyntax.block('<!--', '-->'),
  'ocaml': WorkspaceEditorCommentSyntax.block('(*', '*)'),
  'twig': WorkspaceEditorCommentSyntax.block('{#', '#}'),
  'vue': WorkspaceEditorCommentSyntax.block('<!--', '-->'),
  'xml': WorkspaceEditorCommentSyntax.block('<!--', '-->'),
  'xquery': WorkspaceEditorCommentSyntax.block('(:', ':)'),
};

@visibleForTesting
WorkspaceEditorCommentEdit? workspaceEditorToggleCommentEdit({
  required String text,
  required TextSelection selection,
  required WorkspaceEditorCommentSyntax syntax,
}) {
  if (!selection.isValid ||
      selection.start < 0 ||
      selection.end > text.length) {
    return null;
  }

  final selectionStart = selection.start;
  var targetEnd = selection.end;
  if (!selection.isCollapsed &&
      targetEnd > selectionStart &&
      text[targetEnd - 1] == '\n') {
    targetEnd -= 1;
  }

  final lineStart = selectionStart == 0
      ? 0
      : text.lastIndexOf('\n', selectionStart - 1) + 1;
  var lineEnd = text.indexOf('\n', targetEnd);
  if (lineEnd == -1) lineEnd = text.length;

  final sourceBlock = text.substring(lineStart, lineEnd);
  final sourceLines = sourceBlock.split('\n');
  final actOnBlankLine = sourceLines.length == 1;
  final activeLineIndexes = <int>[
    for (var index = 0; index < sourceLines.length; index++)
      if (actOnBlankLine || sourceLines[index].trim().isNotEmpty) index,
  ];
  if (activeLineIndexes.isEmpty) return null;

  final linePrefix = syntax.linePrefix;
  final blockStart = syntax.blockStart;
  final blockEnd = syntax.blockEnd;
  final shouldUncomment = linePrefix != null
      ? activeLineIndexes.every(
          (index) =>
              _workspaceEditorHasLineComment(sourceLines[index], linePrefix),
        )
      : blockStart != null && blockEnd != null
      ? activeLineIndexes.every(
          (index) => _workspaceEditorHasBlockComment(
            sourceLines[index],
            blockStart,
            blockEnd,
          ),
        )
      : false;

  final changes = <_WorkspaceEditorCommentOffsetChange>[];
  final targetLines = <String>[];
  var sourceOffset = 0;
  for (var index = 0; index < sourceLines.length; index++) {
    final line = sourceLines[index];
    if (!activeLineIndexes.contains(index)) {
      targetLines.add(line);
      sourceOffset += line.length + (index + 1 < sourceLines.length ? 1 : 0);
      continue;
    }

    final result = linePrefix != null
        ? _workspaceEditorToggleLineComment(
            line: line,
            absoluteLineStart: lineStart + sourceOffset,
            prefix: linePrefix,
            uncomment: shouldUncomment,
          )
        : _workspaceEditorToggleBlockComment(
            line: line,
            absoluteLineStart: lineStart + sourceOffset,
            blockStart: blockStart!,
            blockEnd: blockEnd!,
            uncomment: shouldUncomment,
          );
    targetLines.add(result.line);
    changes.addAll(result.changes);
    sourceOffset += line.length + (index + 1 < sourceLines.length ? 1 : 0);
  }

  changes.sort((left, right) => left.start.compareTo(right.start));
  final replacement = targetLines.join('\n');
  if (replacement == sourceBlock) return null;

  return WorkspaceEditorCommentEdit(
    start: lineStart,
    end: lineEnd,
    replacement: replacement,
    selection: TextSelection(
      baseOffset: _workspaceEditorMapCommentOffset(
        selection.baseOffset,
        changes,
      ),
      extentOffset: _workspaceEditorMapCommentOffset(
        selection.extentOffset,
        changes,
      ),
      affinity: selection.affinity,
      isDirectional: selection.isDirectional,
    ),
  );
}

({String line, List<_WorkspaceEditorCommentOffsetChange> changes})
_workspaceEditorToggleLineComment({
  required String line,
  required int absoluteLineStart,
  required String prefix,
  required bool uncomment,
}) {
  final indentLength = _workspaceEditorIndentLength(line);
  final content = line.substring(indentLength);
  final markerOffset = absoluteLineStart + indentLength;

  if (uncomment) {
    var removeLength = prefix.length;
    if (content.length > removeLength && content[removeLength] == ' ') {
      removeLength += 1;
    }
    return (
      line: line.substring(0, indentLength) + content.substring(removeLength),
      changes: <_WorkspaceEditorCommentOffsetChange>[
        _WorkspaceEditorCommentOffsetChange(
          start: markerOffset,
          removedLength: removeLength,
          insertedLength: 0,
        ),
      ],
    );
  }

  final inserted = content.isEmpty ? prefix : '$prefix ';
  return (
    line: line.substring(0, indentLength) + inserted + content,
    changes: <_WorkspaceEditorCommentOffsetChange>[
      _WorkspaceEditorCommentOffsetChange(
        start: markerOffset,
        removedLength: 0,
        insertedLength: inserted.length,
        shiftAtStart: true,
      ),
    ],
  );
}

({String line, List<_WorkspaceEditorCommentOffsetChange> changes})
_workspaceEditorToggleBlockComment({
  required String line,
  required int absoluteLineStart,
  required String blockStart,
  required String blockEnd,
  required bool uncomment,
}) {
  final indentLength = _workspaceEditorIndentLength(line);
  final content = line.substring(indentLength);
  final contentOffset = absoluteLineStart + indentLength;

  if (!uncomment) {
    if (content.isEmpty) {
      final inserted = '$blockStart$blockEnd';
      return (
        line: line + inserted,
        changes: <_WorkspaceEditorCommentOffsetChange>[
          _WorkspaceEditorCommentOffsetChange(
            start: contentOffset,
            removedLength: 0,
            insertedLength: inserted.length,
            shiftAtStart: true,
          ),
        ],
      );
    }
    final prefix = '$blockStart ';
    final suffix = ' $blockEnd';
    return (
      line: line.substring(0, indentLength) + prefix + content + suffix,
      changes: <_WorkspaceEditorCommentOffsetChange>[
        _WorkspaceEditorCommentOffsetChange(
          start: contentOffset,
          removedLength: 0,
          insertedLength: prefix.length,
          shiftAtStart: true,
        ),
        _WorkspaceEditorCommentOffsetChange(
          start: absoluteLineStart + line.length,
          removedLength: 0,
          insertedLength: suffix.length,
        ),
      ],
    );
  }

  var trimmedEnd = content.length;
  while (trimmedEnd > 0 &&
      _workspaceEditorIsInlineWhitespace(content[trimmedEnd - 1])) {
    trimmedEnd -= 1;
  }
  final endMarkerStart = trimmedEnd - blockEnd.length;
  var startRemoveLength = blockStart.length;
  if (content.length > startRemoveLength && content[startRemoveLength] == ' ') {
    startRemoveLength += 1;
  }
  var innerEnd = endMarkerStart;
  if (innerEnd > startRemoveLength && content[innerEnd - 1] == ' ') {
    innerEnd -= 1;
  }
  final trailingWhitespace = content.substring(trimmedEnd);
  final uncommented =
      content.substring(startRemoveLength, innerEnd) + trailingWhitespace;
  return (
    line: line.substring(0, indentLength) + uncommented,
    changes: <_WorkspaceEditorCommentOffsetChange>[
      _WorkspaceEditorCommentOffsetChange(
        start: contentOffset,
        removedLength: startRemoveLength,
        insertedLength: 0,
      ),
      _WorkspaceEditorCommentOffsetChange(
        start: contentOffset + innerEnd,
        removedLength: trimmedEnd - innerEnd,
        insertedLength: 0,
      ),
    ],
  );
}

bool _workspaceEditorHasLineComment(String line, String prefix) {
  final indentLength = _workspaceEditorIndentLength(line);
  return line.substring(indentLength).startsWith(prefix);
}

bool _workspaceEditorHasBlockComment(
  String line,
  String blockStart,
  String blockEnd,
) {
  final indentLength = _workspaceEditorIndentLength(line);
  final content = line.substring(indentLength).trimRight();
  return content.startsWith(blockStart) &&
      content.endsWith(blockEnd) &&
      content.length >= blockStart.length + blockEnd.length;
}

int _workspaceEditorIndentLength(String line) {
  var index = 0;
  while (index < line.length &&
      _workspaceEditorIsInlineWhitespace(line[index])) {
    index += 1;
  }
  return index;
}

bool _workspaceEditorIsInlineWhitespace(String character) =>
    character == ' ' || character == '\t';

int _workspaceEditorMapCommentOffset(
  int offset,
  List<_WorkspaceEditorCommentOffsetChange> changes,
) {
  var delta = 0;
  for (final change in changes) {
    final start = change.start;
    final end = start + change.removedLength;
    if (change.removedLength == 0) {
      if (offset > start || (offset == start && change.shiftAtStart)) {
        delta += change.insertedLength;
      }
      continue;
    }
    if (offset >= end) {
      delta += change.insertedLength - change.removedLength;
      continue;
    }
    if (offset > start || (offset == start && change.shiftAtStart)) {
      return start + delta + change.insertedLength;
    }
  }
  return offset + delta;
}
