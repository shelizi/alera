part of 'workspace_editor_surface.dart';

/// A short debounce avoids rescanning after every pixel of a drag selection.
/// Unlike FindController, this feature never modifies the active search query.
const Duration workspaceEditorSelectionOccurrenceDebounce = Duration(
  milliseconds: 120,
);

const int workspaceEditorMaxOccurrenceHighlights = 400;
const int workspaceEditorMaxSelectedIdentifierScalars = 64;
const int workspaceEditorLargeFileOccurrenceRadiusLines = 160;
const int workspaceEditorMaxOccurrenceLineCodeUnits = 16 * 1024;

extension _WorkspaceEditorSelectionOccurrences on _WorkspaceEditorSurfaceState {
  void _clearSelectedOccurrences() {
    _selectionOccurrenceTimer?.cancel();
    _selectionOccurrenceTimer = null;
    final selection = _controller.selection;
    _lastOccurrenceSelectionStart = selection.start;
    _lastOccurrenceSelectionEnd = selection.end;
    _lastOccurrenceDocumentVersion = _controller.documentVersion;
    _controller.setSelectionOccurrenceHighlights(const []);
  }

  void _scheduleSelectedOccurrencesForSelection() {
    final selection = _controller.selection;
    final version = _controller.documentVersion;
    if (_lastOccurrenceSelectionStart == selection.start &&
        _lastOccurrenceSelectionEnd == selection.end &&
        _lastOccurrenceDocumentVersion == version) {
      return;
    }
    _lastOccurrenceSelectionStart = selection.start;
    _lastOccurrenceSelectionEnd = selection.end;
    _lastOccurrenceDocumentVersion = version;
    _selectionOccurrenceTimer?.cancel();

    if (!selection.isValid ||
        selection.isCollapsed ||
        _loading ||
        _loadError != null ||
        selection.end - selection.start >
            workspaceEditorMaxSelectedIdentifierScalars) {
      _controller.setSelectionOccurrenceHighlights(const []);
      return;
    }

    _selectionOccurrenceTimer = Timer(
      workspaceEditorSelectionOccurrenceDebounce,
      () {
        if (!mounted ||
            _loading ||
            _loadError != null ||
            _controller.documentVersion != version ||
            _controller.selection.start != selection.start ||
            _controller.selection.end != selection.end) {
          return;
        }
        final highlights = workspaceEditorSelectedTextOccurrences(
          selection: selection,
          lineCount: _controller.lineCount,
          documentLength: _controller.length,
          lineForOffset: _controller.getLineAtOffset,
          lineStartOffset: _controller.getLineStartOffset,
          lineText: _controller.getLineText,
          largeFileMode: workspaceEditorShouldDeferDocumentSnapshot(
            lineCount: _controller.lineCount,
            contentLength: _controller.length,
          ),
        );
        _controller.setSelectionOccurrenceHighlights(highlights);
      },
    );
  }
}

final RegExp _editorIdentifierPattern = RegExp(
  r'^[_$\p{L}][_$\p{L}\p{N}]*$',
  unicode: true,
);
final RegExp _editorIdentifierPartPattern = RegExp(
  r'^[_$\p{L}\p{N}]$',
  unicode: true,
);

bool _editorIsIdentifierPartAt(String line, int index) {
  if (index < 0 || index >= line.length) return false;
  var end = index + 1;
  if (index + 1 < line.length) {
    final current = line.codeUnitAt(index);
    final following = line.codeUnitAt(index + 1);
    if (current >= 0xD800 &&
        current <= 0xDBFF &&
        following >= 0xDC00 &&
        following <= 0xDFFF) {
      end++;
    }
  }
  return _editorIdentifierPartPattern.hasMatch(line.substring(index, end));
}

bool _editorIsIdentifierPartBefore(String line, int index) {
  if (index <= 0) return false;
  var start = index - 1;
  if (start > 0) {
    final current = line.codeUnitAt(start);
    final previous = line.codeUnitAt(start - 1);
    if (current >= 0xDC00 &&
        current <= 0xDFFF &&
        previous >= 0xD800 &&
        previous <= 0xDBFF) {
      start--;
    }
  }
  return _editorIdentifierPartPattern.hasMatch(line.substring(start, index));
}

/// Finds whole identifiers and literal selected phrases without ever
/// materializing the complete Rope. Offsets in CodeForge are Unicode scalars,
/// whereas Dart String.indexOf returns UTF-16 code-unit offsets.
///
/// For large files a bounded neighborhood around the selected line is searched
/// instead of scanning megabytes of source on every selection gesture.
@visibleForTesting
List<code_forge.DocumentHighlight> workspaceEditorSelectedTextOccurrences({
  required TextSelection selection,
  required int lineCount,
  required int documentLength,
  required int Function(int) lineForOffset,
  required int Function(int) lineStartOffset,
  required String Function(int) lineText,
  required bool largeFileMode,
  int maxMatches = workspaceEditorMaxOccurrenceHighlights,
}) {
  if (!selection.isValid ||
      selection.isCollapsed ||
      lineCount <= 0 ||
      selection.start < 0 ||
      selection.end > documentLength ||
      selection.end - selection.start >
          workspaceEditorMaxSelectedIdentifierScalars ||
      maxMatches <= 0) {
    return const [];
  }

  final selectedLineIndex = lineForOffset(selection.start);
  if (selectedLineIndex < 0 || selectedLineIndex >= lineCount) return const [];
  final selectedLine = lineText(selectedLineIndex);
  // Avoid shaping or searching pathological single-line files.
  if (selectedLine.length > workspaceEditorMaxOccurrenceLineCodeUnits) {
    return const [];
  }
  final lineStart = lineStartOffset(selectedLineIndex);
  final selectedStartScalar = selection.start - lineStart;
  final selectedEndScalar = selection.end - lineStart;
  if (selectedStartScalar < 0 ||
      selectedEndScalar <= selectedStartScalar ||
      selectedEndScalar > selectedLine.runes.length) {
    return const [];
  }
  final startUtf16 = code_forge.CodeForgeController.scalarToStringIndex(
    selectedLine,
    selectedStartScalar,
  );
  final endUtf16 = code_forge.CodeForgeController.scalarToStringIndex(
    selectedLine,
    selectedEndScalar,
  );
  final selectedText = selectedLine.substring(startUtf16, endUtf16);
  if (selectedText.trim().isEmpty) {
    return const [];
  }
  final wholeIdentifier = _editorIdentifierPattern.hasMatch(selectedText);
  if (wholeIdentifier &&
      (_editorIsIdentifierPartBefore(selectedLine, startUtf16) ||
          _editorIsIdentifierPartAt(selectedLine, endUtf16))) {
    return const []; // Avoid treating part of a larger identifier as a word.
  }
  if (!wholeIdentifier && selectedText.runes.length < 2) {
    return const []; // Ignore one-character punctuation selections.
  }

  final startLine = largeFileMode
      ? (selectedLineIndex - workspaceEditorLargeFileOccurrenceRadiusLines)
            .clamp(0, lineCount - 1)
      : 0;
  final endLine = largeFileMode
      ? (selectedLineIndex + workspaceEditorLargeFileOccurrenceRadiusLines)
            .clamp(0, lineCount - 1)
      : lineCount - 1;

  final occurrences = <code_forge.DocumentHighlight>[];
  final selectedScalarLength = selectedText.runes.length;
  for (var lineIndex = startLine; lineIndex <= endLine; lineIndex++) {
    final source = lineIndex == selectedLineIndex
        ? selectedLine
        : lineText(lineIndex);
    if (source.length > workspaceEditorMaxOccurrenceLineCodeUnits ||
        source.length < selectedText.length) {
      continue;
    }
    var nextIndex = 0;
    while (nextIndex < source.length) {
      final index = source.indexOf(selectedText, nextIndex);
      if (index < 0) break;
      final endIndex = index + selectedText.length;
      nextIndex = endIndex;
      if (wholeIdentifier &&
          (_editorIsIdentifierPartBefore(source, index) ||
              _editorIsIdentifierPartAt(source, endIndex))) {
        continue;
      }
      final scalarColumn = code_forge.CodeForgeController.utf16ToScalarOffset(
        source,
        index,
      );
      if (lineIndex == selectedLineIndex &&
          scalarColumn == selectedStartScalar) {
        continue; // The active selection is already painted by CodeForge.
      }
      occurrences.add(
        code_forge.DocumentHighlight(
          startLine: lineIndex,
          startColumn: scalarColumn,
          endLine: lineIndex,
          endColumn: scalarColumn + selectedScalarLength,
        ),
      );
      if (occurrences.length >= maxMatches) return occurrences;
    }
  }
  return occurrences;
}
