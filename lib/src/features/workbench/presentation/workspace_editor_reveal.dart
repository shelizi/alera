part of 'workspace_editor_surface.dart';

extension _WorkspaceEditorReveal on _WorkspaceEditorSurfaceState {
  Future<void> _reloadFromDiskAfterExternalChange() async {
    if (_isDirty()) {
      return;
    }
    final scrollPosition = _captureEditorScrollPosition();
    _autosave.cancelPending();
    _document.clearSnapshot();
    await _load(restoreScrollPosition: scrollPosition);
  }

  _WorkspaceEditorScrollPosition _captureEditorScrollPosition() {
    return _WorkspaceEditorScrollPosition(
      verticalOffset: _editorScrollOffset(_verticalScrollController),
      horizontalOffset: _editorScrollOffset(_horizontalScrollController),
    );
  }

  double? _editorScrollOffset(ScrollController controller) {
    return controller.hasClients ? controller.offset : null;
  }

  /// Records where the user is in this tab before the surface goes away, so
  /// switching back restores it instead of starting at the top.
  void _rememberEditorViewState() {
    if (_loading || _loadError != null || widget.tab.filePath == null) {
      return;
    }
    final selection = _controller.selection;
    _document.viewState = EditorViewState(
      verticalOffset: _editorScrollOffset(_verticalScrollController),
      horizontalOffset: _editorScrollOffset(_horizontalScrollController),
      selectionBase: selection.isValid ? selection.baseOffset : null,
      selectionExtent: selection.isValid ? selection.extentOffset : null,
    );
  }

  /// The remembered position for this tab, unless a pending reveal (Go to
  /// Definition, search result) asked to land somewhere specific.
  _WorkspaceEditorScrollPosition? _rememberedEditorScrollPosition() {
    final view = _document.viewState;
    if (view == null || _document.pendingReveal != null) {
      return null;
    }
    return _WorkspaceEditorScrollPosition(
      verticalOffset: view.verticalOffset,
      horizontalOffset: view.horizontalOffset,
      selectionBase: view.selectionBase,
      selectionExtent: view.selectionExtent,
    );
  }

  void _scheduleEditorScrollRestore(
    _WorkspaceEditorScrollPosition scrollPosition, {
    required int requestId,
    int attempt = 0,
  }) {
    SchedulerBinding.instance.addPostFrameCallback((_) {
      if (!mounted ||
          requestId != _loadRequestId ||
          _loading ||
          _loadError != null) {
        return;
      }
      if (attempt == 0) {
        _restoreEditorSelection(scrollPosition);
      }
      final verticalDone = _restoreEditorScrollOffset(
        _verticalScrollController,
        scrollPosition.verticalOffset,
      );
      final horizontalDone = _restoreEditorScrollOffset(
        _horizontalScrollController,
        scrollPosition.horizontalOffset,
      );
      // The editor can take several frames to lay out a long document; until
      // then the extent is too short and the offset would be clamped short.
      if ((!verticalDone || !horizontalDone) &&
          attempt < _workspaceEditorScrollRestoreAttempts) {
        _scheduleEditorScrollRestore(
          scrollPosition,
          requestId: requestId,
          attempt: attempt + 1,
        );
        SchedulerBinding.instance.scheduleFrame();
      }
    });
  }

  void _restoreEditorSelection(_WorkspaceEditorScrollPosition position) {
    final base = position.selectionBase;
    final extent = position.selectionExtent;
    if (base == null || extent == null) {
      return;
    }
    final length = _controller.length;
    _controller.selection = TextSelection(
      baseOffset: base.clamp(0, length).toInt(),
      extentOffset: extent.clamp(0, length).toInt(),
    );
  }

  /// Returns whether [offset] was reached (or nothing is left to restore).
  bool _restoreEditorScrollOffset(ScrollController controller, double? offset) {
    if (offset == null) {
      return true;
    }
    if (!controller.hasClients) {
      return false;
    }
    final position = controller.position;
    final target = offset
        .clamp(position.minScrollExtent, position.maxScrollExtent)
        .toDouble();
    if (position.pixels != target) {
      controller.jumpTo(target);
    }
    return target == offset;
  }

  void _revealOrDefer(WorkspaceEditorRevealTarget target) {
    if (_loading || _loadError != null) {
      _document.pendingReveal = target;
      return;
    }
    _reveal(target);
  }

  void _applyPendingReveal() {
    final target = _editorSessions.takePendingReveal(widget.tab.id);
    if (target != null) {
      _revealOrDefer(target);
    }
  }

  void _reveal(WorkspaceEditorRevealTarget target) {
    if (!mounted || _controller.lineCount == 0) {
      return;
    }
    final lineIndex = (target.line - 1)
        .clamp(0, _controller.lineCount - 1)
        .toInt();
    final lineText = _controller.getLineText(lineIndex);
    final displayRange = workspaceEditorDisplayRevealRange(
      rawText: _document.loadedRawText,
      lineNumber: target.line,
      rawColumn: target.column,
      rawMatchLength: target.matchLength,
      tabSize: _currentEditorTabSize(),
    );
    final columnIndex = displayRange.columnIndex.clamp(0, lineText.length);
    final startOffset = _controller.getLineStartOffset(lineIndex) + columnIndex;
    final endOffset = (startOffset + displayRange.matchLength)
        .clamp(startOffset, _controller.length)
        .toInt();
    _controller.selection = TextSelection(
      baseOffset: startOffset,
      extentOffset: endOffset,
    );
    SchedulerBinding.instance.addPostFrameCallback((_) {
      if (!mounted) {
        return;
      }
      try {
        _controller.scrollToLine(lineIndex);
      } on StateError {
        _document.pendingReveal = target;
      }
      _focusNode.requestFocus();
    });
  }
}

/// About half a second at 60 Hz: enough for a large native document to lay
/// out, short enough that a document that shrank does not keep retrying.
const int _workspaceEditorScrollRestoreAttempts = 30;

class const _WorkspaceEditorScrollPosition({
  final double? verticalOffset,
  final double? horizontalOffset,
  final int? selectionBase,
  final int? selectionExtent,
});

@visibleForTesting
WorkspaceEditorDisplayRevealRange workspaceEditorDisplayRevealRange({
  required String? rawText,
  required int lineNumber,
  required int rawColumn,
  required int rawMatchLength,
  required int tabSize,
}) {
  final rawLine = _rawLineAt(rawText, lineNumber);
  if (rawLine == null) {
    return WorkspaceEditorDisplayRevealRange(
      columnIndex: (rawColumn - 1).clamp(0, rawColumn).toInt(),
      matchLength: rawMatchLength,
    );
  }
  final startOffset = _displayUtf16OffsetForRawColumn(
    rawLine: rawLine,
    rawColumn: rawColumn,
    tabSize: tabSize,
  );
  final endOffset = _displayUtf16OffsetForRawColumn(
    rawLine: rawLine,
    rawColumn: rawColumn + rawMatchLength,
    tabSize: tabSize,
  );
  return WorkspaceEditorDisplayRevealRange(
    columnIndex: startOffset,
    matchLength: (endOffset - startOffset).clamp(0, endOffset).toInt(),
  );
}

@visibleForTesting
class const WorkspaceEditorDisplayRevealRange({
  required final int columnIndex,
  required final int matchLength,
});

String? _rawLineAt(String? rawText, int lineNumber) {
  if (rawText == null || lineNumber < 1) {
    return null;
  }
  var currentLine = 1;
  var start = 0;
  for (var index = 0; index <= rawText.length; index += 1) {
    if (index < rawText.length && rawText.codeUnitAt(index) != 0x0A) {
      continue;
    }
    if (currentLine == lineNumber) {
      var end = index;
      if (end > start && rawText.codeUnitAt(end - 1) == 0x0D) {
        end -= 1;
      }
      return rawText.substring(start, end);
    }
    currentLine += 1;
    start = index + 1;
  }
  return null;
}

int _displayUtf16OffsetForRawColumn({
  required String rawLine,
  required int rawColumn,
  required int tabSize,
}) {
  final effectiveTabSize = tabSize.clamp(1, 8).toInt();
  final rawLength = rawLine.runes.length;
  final rawIndexLimit = (rawColumn - 1).clamp(0, rawLength).toInt();
  var rawIndex = 0;
  var visualColumn = 0;
  var utf16Offset = 0;
  for (final rune in rawLine.runes) {
    if (rawIndex >= rawIndexLimit) {
      break;
    }
    rawIndex += 1;
    if (rune == 0x09) {
      final spaces = effectiveTabSize - (visualColumn % effectiveTabSize);
      visualColumn += spaces;
      utf16Offset += spaces;
    } else {
      visualColumn += 1;
      utf16Offset += String.fromCharCode(rune).length;
    }
  }
  return utf16Offset;
}

class const _EditorMessage({required final String message})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Center(
      child: Text(
        message,
        style: Theme.of(context).textTheme.bodyMedium
            ?.copyWith(color: AleraTokens.foregroundMuted),
      ),
    );
  }
}
