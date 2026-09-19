part of 'workspace_git_diff_surface.dart';

final class _EditableWorkingTreeDocument {
  _EditableWorkingTreeDocument({
    required this.relativePath,
    required this.loaded,
  }) : currentText = loaded.displayContent;

  final String relativePath;
  native.WorkspaceEditorTextFile loaded;
  String currentText;
  bool saving = false;
  Object? error;

  bool get dirty => currentText != loaded.displayContent;
}

final class _LiveDiffStats {
  const _LiveDiffStats({required this.added, required this.removed});

  final int added;
  final int removed;
}

final class _EditableDiffLineChanges {
  const _EditableDiffLineChanges({
    required this.oldChangedLines,
    required this.newChangedLines,
  });

  final Set<int> oldChangedLines;
  final Set<int> newChangedLines;
}

_LiveDiffStats _liveDiffStats(
  String baseline,
  String current,
  GitDiffWhitespaceMode whitespaceMode,
) {
  final oldLines = _splitFullFileLines(baseline)
      .map((line) => _normalizeLiveDiffLine(line, whitespaceMode))
      .toList(growable: false);
  final newLines = _splitFullFileLines(current)
      .map((line) => _normalizeLiveDiffLine(line, whitespaceMode))
      .toList(growable: false);
  var prefix = 0;
  while (prefix < oldLines.length &&
      prefix < newLines.length &&
      oldLines[prefix] == newLines[prefix]) {
    prefix += 1;
  }
  var oldSuffix = oldLines.length;
  var newSuffix = newLines.length;
  while (oldSuffix > prefix &&
      newSuffix > prefix &&
      oldLines[oldSuffix - 1] == newLines[newSuffix - 1]) {
    oldSuffix -= 1;
    newSuffix -= 1;
  }
  return _LiveDiffStats(added: newSuffix - prefix, removed: oldSuffix - prefix);
}

String _normalizeLiveDiffLine(String line, GitDiffWhitespaceMode mode) {
  return switch (mode) {
    GitDiffWhitespaceMode.normal => line,
    GitDiffWhitespaceMode.ignoreEol => line.replaceFirst(
      RegExp(r'[ \t]+$'),
      '',
    ),
    GitDiffWhitespaceMode.ignoreChanges =>
      line.replaceAll(RegExp(r'[ \t]+'), ' ').replaceFirst(RegExp(r' $'), ''),
    GitDiffWhitespaceMode.ignoreAll => line.replaceAll(RegExp(r'[ \t]+'), ''),
  };
}

_EditableDiffLineChanges _liveEditableDiffLineChanges(
  String baseline,
  String current,
  GitDiffWhitespaceMode whitespaceMode,
) {
  final oldLines = _splitFullFileLines(baseline)
      .map((line) => _normalizeLiveDiffLine(line, whitespaceMode))
      .toList(growable: false);
  final newLines = _splitFullFileLines(current)
      .map((line) => _normalizeLiveDiffLine(line, whitespaceMode))
      .toList(growable: false);
  var prefix = 0;
  while (prefix < oldLines.length &&
      prefix < newLines.length &&
      oldLines[prefix] == newLines[prefix]) {
    prefix += 1;
  }
  var oldSuffix = oldLines.length;
  var newSuffix = newLines.length;
  while (oldSuffix > prefix &&
      newSuffix > prefix &&
      oldLines[oldSuffix - 1] == newLines[newSuffix - 1]) {
    oldSuffix -= 1;
    newSuffix -= 1;
  }
  return _EditableDiffLineChanges(
    oldChangedLines: Set<int>.from(
      Iterable<int>.generate(oldSuffix - prefix, (index) => prefix + index),
    ),
    newChangedLines: Set<int>.from(
      Iterable<int>.generate(newSuffix - prefix, (index) => prefix + index),
    ),
  );
}

_EditableDiffLineChanges _editableDiffLineChanges(
  GitDiffFile file,
  String baseline,
  String current,
  GitDiffWhitespaceMode whitespaceMode, {
  required bool useSourceDiff,
}) {
  if (!useSourceDiff) {
    return _liveEditableDiffLineChanges(baseline, current, whitespaceMode);
  }

  final oldLines = _splitFullFileLines(baseline);
  final newLines = _splitFullFileLines(current);
  if (file.status == GitChangeStatus.added ||
      file.status == GitChangeStatus.untracked) {
    return _EditableDiffLineChanges(
      oldChangedLines: const <int>{},
      newChangedLines: Set<int>.from(Iterable<int>.generate(newLines.length)),
    );
  }

  final oldChanged = <int>{};
  final newChanged = <int>{};
  int? oldLine;
  int? newLine;
  for (final line in file.lines) {
    if (line.kind == GitDiffLineKind.hunk) {
      final match = _hunkHeaderRegExp.firstMatch(line.text);
      oldLine = int.tryParse(match?.group(1) ?? '');
      newLine = int.tryParse(match?.group(3) ?? '');
      if (match?.group(2) == '0') oldLine = null;
      if (match?.group(4) == '0') newLine = null;
      continue;
    }
    if (line.kind == GitDiffLineKind.header) continue;
    if (line.kind == GitDiffLineKind.deletion) {
      if (oldLine != null && oldLine > 0 && oldLine <= oldLines.length) {
        oldChanged.add(oldLine - 1);
      }
      if (oldLine != null) oldLine += 1;
      continue;
    }
    if (line.kind == GitDiffLineKind.addition) {
      if (newLine != null && newLine > 0 && newLine <= newLines.length) {
        newChanged.add(newLine - 1);
      }
      if (newLine != null) newLine += 1;
      continue;
    }
    if (line.kind == GitDiffLineKind.context) {
      if (oldLine != null) oldLine += 1;
      if (newLine != null) newLine += 1;
    }
  }

  if (oldChanged.isNotEmpty || newChanged.isNotEmpty || baseline == current) {
    return _EditableDiffLineChanges(
      oldChangedLines: oldChanged,
      newChangedLines: newChanged,
    );
  }
  return _liveEditableDiffLineChanges(baseline, current, whitespaceMode);
}

extension _WorkspaceGitDiffEditable on _WorkspaceGitDiffSurfaceState {
  bool _canEditWorkingTreeFile(GitDiffFile file) {
    return widget.tab.gitDiffSource == WorkspaceGitDiffSource.workingTree &&
        file.area != GitChangeArea.staged &&
        (file.area == GitChangeArea.unstaged ||
            file.area == GitChangeArea.untracked) &&
        file.status != GitChangeStatus.deleted &&
        !file.isBinary &&
        !file.isLarge &&
        !file.isGitlink;
  }

  Future<void> _ensureEditableDocument({
    required GitDiffFile file,
    required WorkspaceSourceControlScope sourceControlScope,
    required int loadGeneration,
  }) async {
    if (!_canEditWorkingTreeFile(file)) return;
    final relativePath = sourceControlScope.toWorkspaceRelativePath(file.path);
    if (relativePath == null || _editableDocuments.containsKey(file.path)) {
      return;
    }
    try {
      final tabSize = ref
          .read(settingsControllerProvider)
          .editor
          .tabSize
          .clamp(1, 8)
          .toInt();
      final loaded = await ref
          .read(workspaceFileServiceProvider)
          .readEditorTextFile(
            workspacePath: widget.workspace.path,
            relativePath: relativePath,
            tabSize: tabSize,
            encoding: _encodingSelection.encoding,
          );
      if (!mounted || loadGeneration != _diffLoadGeneration) return;
      _updateDiffState(() {
        _editableDocuments[file.path] = _EditableWorkingTreeDocument(
          relativePath: relativePath,
          loaded: loaded,
        );
      });
    } catch (_) {
      // The ordinary read-only diff remains available when the workspace file
      // cannot be opened through the editor pipeline.
    }
  }

  void _editWorkingTreeDocument(GitDiffFile file, String text) {
    final document = _editableDocuments[file.path];
    if (document == null || document.currentText == text) return;
    _updateDiffState(() {
      document.currentText = text;
      document.error = null;
    });
  }

  Future<void> _saveWorkingTreeDocument(GitDiffFile file) async {
    final document = _editableDocuments[file.path];
    if (document == null || document.saving || !document.dirty) return;
    _updateDiffState(() {
      document.saving = true;
      document.error = null;
    });
    final contentBeingSaved = document.currentText;
    try {
      final saved = await _writeWorkingTreeDocument(
        document,
        overwriteIfChanged: false,
      );
      if (!mounted) return;
      final unchanged = _acceptWorkingTreeSave(
        document,
        saved,
        contentBeingSaved: contentBeingSaved,
      );
      if (unchanged) {
        _load();
      }
    } catch (error) {
      if (!mounted) return;
      if (error is native.WorkspaceFileError &&
          error.kind == native.WorkspaceFileErrorKind.conflict) {
        final overwrite = await showDialog<bool>(
          context: context,
          builder: (context) => const AleraConfirmDialog(
            title: 'File changed on disk',
            message: 'Overwrite the file with the diff editor contents?',
            confirmLabel: 'Overwrite',
            destructive: true,
          ),
        );
        if (overwrite == true && mounted) {
          final overwriteContent = document.currentText;
          try {
            final saved = await _writeWorkingTreeDocument(
              document,
              overwriteIfChanged: true,
            );
            if (!mounted) return;
            final unchanged = _acceptWorkingTreeSave(
              document,
              saved,
              contentBeingSaved: overwriteContent,
            );
            if (unchanged) {
              _load();
            }
          } catch (overwriteError) {
            if (mounted) {
              _updateDiffState(() => document.error = overwriteError);
            }
          }
        }
      } else {
        _updateDiffState(() => document.error = error);
      }
    } finally {
      if (mounted && identical(_editableDocuments[file.path], document)) {
        _updateDiffState(() => document.saving = false);
      }
    }
  }

  Future<native.WorkspaceEditorTextFile> _writeWorkingTreeDocument(
    _EditableWorkingTreeDocument document, {
    required bool overwriteIfChanged,
  }) {
    final tabSize = ref
        .read(settingsControllerProvider)
        .editor
        .tabSize
        .clamp(1, 8)
        .toInt();
    return ref
        .read(workspaceFileServiceProvider)
        .writeEditorTextFile(
          workspacePath: widget.workspace.path,
          relativePath: document.relativePath,
          currentDisplayContent: document.currentText,
          originalRawContent: document.loaded.rawContent,
          originalDisplayContent: document.loaded.displayContent,
          expectedContentToken: document.loaded.contentToken,
          overwriteIfChanged: overwriteIfChanged,
          tabSize: tabSize,
          encoding: document.loaded.encoding,
        );
  }

  bool _acceptWorkingTreeSave(
    _EditableWorkingTreeDocument document,
    native.WorkspaceEditorTextFile saved, {
    required String contentBeingSaved,
  }) {
    final unchanged = document.currentText == contentBeingSaved;
    document.loaded = saved;
    if (unchanged) {
      document.currentText = saved.displayContent;
    }
    document.error = null;
    return unchanged;
  }
}

class _EditableWorkingTreeDiff extends StatefulWidget {
  const _EditableWorkingTreeDiff({
    required this.file,
    required this.baseline,
    required this.document,
    required this.whitespaceMode,
    this.viewportHeight,
    required this.onChanged,
    required this.onSave,
  });

  final GitDiffFile file;
  final String baseline;
  final _EditableWorkingTreeDocument document;
  final GitDiffWhitespaceMode whitespaceMode;
  final double? viewportHeight;
  final ValueChanged<String> onChanged;
  final VoidCallback onSave;

  @override
  State<_EditableWorkingTreeDiff> createState() =>
      _EditableWorkingTreeDiffState();
}

class _EditableDiffOverviewRuler extends StatelessWidget {
  const _EditableDiffOverviewRuler({
    super.key,
    required this.changes,
    required this.lineCount,
    required this.scrollController,
  });

  static const double width = 14;

  final _EditableDiffLineChanges changes;
  final int lineCount;
  final ScrollController scrollController;

  void _jumpTo(double localY, double height) {
    if (!scrollController.hasClients || height <= 0) return;
    final position = scrollController.position;
    final ratio = (localY / height).clamp(0.0, 1.0);
    final target = (ratio * position.maxScrollExtent).clamp(
      position.minScrollExtent,
      position.maxScrollExtent,
    );
    scrollController.jumpTo(target);
  }

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: 'Diff Overview',
      child: Semantics(
        button: true,
        label:
            'Diff overview, ${changes.oldChangedLines.length} removed, '
            '${changes.newChangedLines.length} added',
        child: DecoratedBox(
          decoration: const BoxDecoration(
            color: AleraTokens.surfaceVariant,
            border: Border(left: BorderSide(color: AleraTokens.borderSubtle)),
          ),
          child: LayoutBuilder(
            builder: (context, constraints) {
              final height = constraints.maxHeight;
              return MouseRegion(
                cursor: SystemMouseCursors.click,
                child: GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onTapDown: (details) =>
                      _jumpTo(details.localPosition.dy, height),
                  onVerticalDragStart: (details) =>
                      _jumpTo(details.localPosition.dy, height),
                  onVerticalDragUpdate: (details) =>
                      _jumpTo(details.localPosition.dy, height),
                  child: SizedBox(
                    width: width,
                    child: AnimatedBuilder(
                      animation: scrollController,
                      builder: (context, _) {
                        var viewportStart = 0.0;
                        var viewportEnd = 1.0;
                        if (scrollController.hasClients) {
                          final position = scrollController.position;
                          final totalExtent =
                              position.maxScrollExtent +
                              position.viewportDimension;
                          if (totalExtent > 0) {
                            viewportStart = (position.pixels / totalExtent)
                                .clamp(0.0, 1.0);
                            viewportEnd =
                                ((position.pixels +
                                            position.viewportDimension) /
                                        totalExtent)
                                    .clamp(viewportStart, 1.0);
                          }
                        }
                        return CustomPaint(
                          painter: _EditableDiffOverviewPainter(
                            oldChangedLines: changes.oldChangedLines,
                            newChangedLines: changes.newChangedLines,
                            lineCount: lineCount,
                            viewportStart: viewportStart,
                            viewportEnd: viewportEnd,
                          ),
                        );
                      },
                    ),
                  ),
                ),
              );
            },
          ),
        ),
      ),
    );
  }
}

class _EditableDiffOverviewPainter extends CustomPainter {
  const _EditableDiffOverviewPainter({
    required this.oldChangedLines,
    required this.newChangedLines,
    required this.lineCount,
    required this.viewportStart,
    required this.viewportEnd,
  });

  final Set<int> oldChangedLines;
  final Set<int> newChangedLines;
  final int lineCount;
  final double viewportStart;
  final double viewportEnd;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.width <= 0 || size.height <= 0) return;

    final contentWidth = math.max(0.0, size.width - 4);
    final halfWidth = contentWidth / 2;
    final markerHeight = math.max(
      2.0,
      math.min(5.0, size.height / math.max(lineCount, 1) * 1.5),
    );
    final denominator = math.max(lineCount - 1, 1);

    double markerTop(int lineIndex) {
      final ratio = (lineIndex / denominator).clamp(0.0, 1.0);
      return ratio * math.max(0.0, size.height - markerHeight);
    }

    final removedPaint = Paint()..color = AleraTokens.error;
    final addedPaint = Paint()..color = AleraTokens.success;
    for (final lineIndex in oldChangedLines) {
      canvas.drawRect(
        Rect.fromLTWH(2, markerTop(lineIndex), halfWidth, markerHeight),
        removedPaint,
      );
    }
    for (final lineIndex in newChangedLines) {
      canvas.drawRect(
        Rect.fromLTWH(
          2 + halfWidth,
          markerTop(lineIndex),
          halfWidth,
          markerHeight,
        ),
        addedPaint,
      );
    }

    final top = viewportStart.clamp(0.0, 1.0) * size.height;
    final bottom = viewportEnd.clamp(viewportStart, 1.0) * size.height;
    final viewportRect = Rect.fromLTRB(
      1,
      top,
      size.width - 1,
      math.max(top + 4, bottom).clamp(0.0, size.height),
    );
    canvas.drawRect(
      viewportRect,
      Paint()..color = AleraTokens.foregroundMuted.withValues(alpha: 0.08),
    );
    canvas.drawRect(
      viewportRect,
      Paint()
        ..color = AleraTokens.foregroundMuted.withValues(alpha: 0.5)
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1,
    );
  }

  @override
  bool shouldRepaint(covariant _EditableDiffOverviewPainter oldDelegate) =>
      oldDelegate.oldChangedLines != oldChangedLines ||
      oldDelegate.newChangedLines != newChangedLines ||
      oldDelegate.lineCount != lineCount ||
      oldDelegate.viewportStart != viewportStart ||
      oldDelegate.viewportEnd != viewportEnd;
}

class _EditableWorkingTreeDiffState extends State<_EditableWorkingTreeDiff> {
  late final TextEditingController _controller;
  late final ScrollController _leftHorizontalController;
  late final ScrollController _rightHorizontalController;
  late final ScrollController _leftVerticalController;
  late final ScrollController _rightVerticalController;
  var _syncingHorizontal = false;
  var _syncingVertical = false;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.document.currentText);
    _leftHorizontalController = ScrollController();
    _rightHorizontalController = ScrollController();
    _leftVerticalController = ScrollController();
    _rightVerticalController = ScrollController();
    _leftHorizontalController.addListener(_syncHorizontalFromLeft);
    _rightHorizontalController.addListener(_syncHorizontalFromRight);
    _leftVerticalController.addListener(_syncVerticalFromLeft);
    _rightVerticalController.addListener(_syncVerticalFromRight);
  }

  @override
  void didUpdateWidget(covariant _EditableWorkingTreeDiff oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (_controller.text != widget.document.currentText &&
        oldWidget.document.currentText != widget.document.currentText) {
      _controller.value = TextEditingValue(
        text: widget.document.currentText,
        selection: TextSelection.collapsed(
          offset: widget.document.currentText.length,
        ),
      );
    }
  }

  @override
  void dispose() {
    _leftHorizontalController.dispose();
    _rightHorizontalController.dispose();
    _leftVerticalController.dispose();
    _rightVerticalController.dispose();
    _controller.dispose();
    super.dispose();
  }

  void _syncHorizontalFromLeft() => _syncScrollOffset(
    _leftHorizontalController,
    _rightHorizontalController,
    horizontal: true,
  );

  void _syncHorizontalFromRight() => _syncScrollOffset(
    _rightHorizontalController,
    _leftHorizontalController,
    horizontal: true,
  );

  void _syncVerticalFromLeft() => _syncScrollOffset(
    _leftVerticalController,
    _rightVerticalController,
    horizontal: false,
  );

  void _syncVerticalFromRight() => _syncScrollOffset(
    _rightVerticalController,
    _leftVerticalController,
    horizontal: false,
  );

  void _syncScrollOffset(
    ScrollController source,
    ScrollController target, {
    required bool horizontal,
  }) {
    if (!source.hasClients || !target.hasClients) return;
    if (horizontal ? _syncingHorizontal : _syncingVertical) return;
    final targetPosition = target.position;
    final nextOffset = source.offset.clamp(
      targetPosition.minScrollExtent,
      targetPosition.maxScrollExtent,
    );
    if ((target.offset - nextOffset).abs() < 0.5) return;
    if (horizontal) {
      _syncingHorizontal = true;
    } else {
      _syncingVertical = true;
    }
    try {
      target.jumpTo(nextOffset);
    } finally {
      if (horizontal) {
        _syncingHorizontal = false;
      } else {
        _syncingVertical = false;
      }
    }
  }

  double _sharedContentWidth(BuildContext context, TextStyle textStyle) {
    final painter = TextPainter(
      textDirection: Directionality.of(context),
      maxLines: 1,
    );
    var maxWidth = 0.0;
    for (final content in <String>[
      widget.baseline,
      widget.document.currentText,
    ]) {
      for (final line in _splitFullFileLines(content)) {
        painter.text = TextSpan(
          text: line.isEmpty ? ' ' : line,
          style: textStyle,
        );
        painter.layout();
        maxWidth = math.max(maxWidth, painter.width);
      }
    }
    return maxWidth + AleraTokens.space24;
  }

  @override
  Widget build(BuildContext context) {
    final stats = _liveDiffStats(
      widget.baseline,
      widget.document.currentText,
      widget.whitespaceMode,
    );
    final changes = _editableDiffLineChanges(
      widget.file,
      widget.baseline,
      widget.document.currentText,
      widget.whitespaceMode,
      useSourceDiff:
          widget.document.currentText == widget.document.loaded.displayContent,
    );
    const lineHeight = 18.0;
    final lineCount = math.max(
      _splitFullFileLines(widget.baseline).length,
      _splitFullFileLines(widget.document.currentText).length,
    );
    final fallbackHeight = (lineCount * 18.0 + 72).clamp(260.0, 620.0);
    final viewportHeight = widget.viewportHeight;
    final height =
        viewportHeight != null && viewportHeight.isFinite && viewportHeight > 0
        ? viewportHeight
        : fallbackHeight;
    final textStyle = AleraTokens.monoStyle.copyWith(
      fontSize: 12,
      color: AleraTokens.foreground,
      height: 1.5,
    );
    final sharedContentWidth = _sharedContentWidth(context, textStyle);
    return CallbackShortcuts(
      bindings: <ShortcutActivator, VoidCallback>{
        const SingleActivator(LogicalKeyboardKey.keyS, control: true):
            widget.onSave,
        const SingleActivator(LogicalKeyboardKey.keyS, meta: true):
            widget.onSave,
      },
      child: SizedBox(
        height: height,
        child: Column(
          crossAxisAlignment: .stretch,
          children: <Widget>[
            Container(
              padding: const EdgeInsets.symmetric(
                horizontal: AleraTokens.space12,
                vertical: AleraTokens.space6,
              ),
              decoration: const BoxDecoration(
                color: AleraTokens.surfaceVariant,
                border: Border(
                  bottom: BorderSide(color: AleraTokens.borderSubtle),
                ),
              ),
              child: Row(
                children: <Widget>[
                  const Expanded(child: Text('Original · Read Only')),
                  Expanded(
                    child: Row(
                      children: <Widget>[
                        const Expanded(
                          child: Text(
                            'Workspace · Editable',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                        if (stats.added > 0)
                          Text(
                            '+${stats.added}',
                            style: const TextStyle(color: AleraTokens.success),
                          ),
                        if (stats.removed > 0) ...<Widget>[
                          const SizedBox(width: AleraTokens.space6),
                          Text(
                            '-${stats.removed}',
                            style: const TextStyle(color: AleraTokens.error),
                          ),
                        ],
                        const SizedBox(width: AleraTokens.space8),
                        AleraIconButton(
                          tooltip: widget.document.saving
                              ? 'Saving File'
                              : 'Save File',
                          icon: widget.document.saving
                              ? AleraIcons.loading
                              : AleraIcons.save,
                          onPressed:
                              widget.document.dirty && !widget.document.saving
                              ? widget.onSave
                              : null,
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
            if (widget.document.error != null)
              const _DiffBanner(message: 'Could not save workspace file.'),
            Expanded(
              child: Row(
                crossAxisAlignment: .stretch,
                children: <Widget>[
                  Expanded(
                    child: LayoutBuilder(
                      builder: (context, constraints) {
                        final contentWidth = math.max(
                          constraints.maxWidth,
                          sharedContentWidth,
                        );
                        return DecoratedBox(
                          decoration: const BoxDecoration(
                            border: Border(
                              right: BorderSide(
                                color: AleraTokens.borderSubtle,
                              ),
                            ),
                          ),
                          child: Scrollbar(
                            key: ValueKey<String>(
                              'git-diff-working-tree-original-x-scrollbar-${widget.file.path}',
                            ),
                            controller: _leftHorizontalController,
                            thumbVisibility: true,
                            scrollbarOrientation: ScrollbarOrientation.bottom,
                            notificationPredicate: (notification) =>
                                notification.metrics.axis == Axis.horizontal,
                            child: SingleChildScrollView(
                              controller: _leftHorizontalController,
                              scrollDirection: Axis.horizontal,
                              child: SizedBox(
                                width: contentWidth,
                                height: constraints.maxHeight,
                                child: Scrollbar(
                                  key: ValueKey<String>(
                                    'git-diff-working-tree-original-y-scrollbar-${widget.file.path}',
                                  ),
                                  controller: _leftVerticalController,
                                  thumbVisibility: true,
                                  notificationPredicate: (notification) =>
                                      notification.metrics.axis ==
                                      Axis.vertical,
                                  child: SingleChildScrollView(
                                    controller: _leftVerticalController,
                                    padding: const EdgeInsets.all(
                                      AleraTokens.space8,
                                    ),
                                    child: Stack(
                                      fit: StackFit.passthrough,
                                      children: <Widget>[
                                        for (final lineIndex
                                            in changes.oldChangedLines)
                                          Positioned(
                                            key: ValueKey<String>(
                                              'git-diff-working-tree-original-deletion-${widget.file.path}-$lineIndex',
                                            ),
                                            left: 0,
                                            right: 0,
                                            top: lineIndex * lineHeight,
                                            height: lineHeight,
                                            child: IgnorePointer(
                                              child: DecoratedBox(
                                                decoration: BoxDecoration(
                                                  color: AleraTokens.error
                                                      .withValues(alpha: 0.08),
                                                  border: const Border(
                                                    left: BorderSide(
                                                      color: AleraTokens.error,
                                                      width: 3,
                                                    ),
                                                  ),
                                                ),
                                              ),
                                            ),
                                          ),
                                        SelectableText(
                                          widget.baseline,
                                          style: textStyle,
                                        ),
                                      ],
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          ),
                        );
                      },
                    ),
                  ),
                  Expanded(
                    child: LayoutBuilder(
                      builder: (context, constraints) {
                        final contentWidth = math.max(
                          constraints.maxWidth,
                          sharedContentWidth,
                        );
                        return Scrollbar(
                          key: ValueKey<String>(
                            'git-diff-working-tree-editor-x-scrollbar-${widget.file.path}',
                          ),
                          controller: _rightHorizontalController,
                          thumbVisibility: true,
                          scrollbarOrientation: ScrollbarOrientation.bottom,
                          notificationPredicate: (notification) =>
                              notification.metrics.axis == Axis.horizontal,
                          child: SingleChildScrollView(
                            controller: _rightHorizontalController,
                            scrollDirection: Axis.horizontal,
                            child: SizedBox(
                              width: contentWidth,
                              height: constraints.maxHeight,
                              child: Scrollbar(
                                key: ValueKey<String>(
                                  'git-diff-working-tree-editor-y-scrollbar-${widget.file.path}',
                                ),
                                controller: _rightVerticalController,
                                thumbVisibility: true,
                                notificationPredicate: (notification) =>
                                    notification.metrics.axis == Axis.vertical,
                                child: AnimatedBuilder(
                                  animation: _rightVerticalController,
                                  child: TextField(
                                    key: ValueKey<String>(
                                      'git-diff-working-tree-editor-${widget.file.path}',
                                    ),
                                    controller: _controller,
                                    scrollController: _rightVerticalController,
                                    expands: true,
                                    maxLines: null,
                                    minLines: null,
                                    keyboardType: TextInputType.multiline,
                                    style: textStyle,
                                    decoration: const InputDecoration(
                                      border: InputBorder.none,
                                      filled: false,
                                      contentPadding: EdgeInsets.all(
                                        AleraTokens.space8,
                                      ),
                                    ),
                                    onChanged: (text) {
                                      setState(() {});
                                      widget.onChanged(text);
                                    },
                                  ),
                                  builder: (context, child) {
                                    final scrollOffset =
                                        _rightVerticalController.hasClients
                                        ? _rightVerticalController.offset
                                        : 0.0;
                                    final firstVisible = math.max(
                                      0,
                                      ((scrollOffset - AleraTokens.space8) /
                                                  lineHeight)
                                              .floor() -
                                          1,
                                    );
                                    final lastVisible = math.min(
                                      lineCount - 1,
                                      ((scrollOffset +
                                                  constraints.maxHeight -
                                                  AleraTokens.space8) /
                                              lineHeight)
                                          .ceil(),
                                    );
                                    return Stack(
                                      clipBehavior: Clip.hardEdge,
                                      children: <Widget>[
                                        Positioned.fill(child: child!),
                                        if (lastVisible >= firstVisible)
                                          for (
                                            var lineIndex = firstVisible;
                                            lineIndex <= lastVisible;
                                            lineIndex += 1
                                          )
                                            if (changes.newChangedLines
                                                .contains(lineIndex))
                                              Positioned(
                                                key: ValueKey<String>(
                                                  'git-diff-working-tree-editor-addition-${widget.file.path}-$lineIndex',
                                                ),
                                                left: 0,
                                                right: 0,
                                                top:
                                                    AleraTokens.space8 +
                                                    lineIndex * lineHeight -
                                                    scrollOffset,
                                                height: lineHeight,
                                                child: IgnorePointer(
                                                  child: DecoratedBox(
                                                    decoration: BoxDecoration(
                                                      color: AleraTokens.success
                                                          .withValues(
                                                            alpha: 0.08,
                                                          ),
                                                      border: const Border(
                                                        left: BorderSide(
                                                          color: AleraTokens
                                                              .success,
                                                          width: 3,
                                                        ),
                                                      ),
                                                    ),
                                                  ),
                                                ),
                                              ),
                                      ],
                                    );
                                  },
                                ),
                              ),
                            ),
                          ),
                        );
                      },
                    ),
                  ),
                  SizedBox(
                    width: _EditableDiffOverviewRuler.width,
                    child: _EditableDiffOverviewRuler(
                      key: ValueKey<String>(
                        'git-diff-working-tree-overview-${widget.file.path}',
                      ),
                      changes: changes,
                      lineCount: lineCount,
                      scrollController: _rightVerticalController,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
