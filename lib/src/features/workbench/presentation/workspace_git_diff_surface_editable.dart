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

/// Keeps horizontal diff scrolling under explicit user control.
///
/// EditableText asks ancestor scrollables to reveal the caret/selection. For
/// the side-by-side diff, that implicit horizontal reveal makes the whole pane
/// jump left/right when selection moves between long and short lines. User
/// wheel/drag/scrollbar input remains enabled; only implicit bring-into-view
/// requests are rejected.
final class _NoImplicitHorizontalScrollPhysics extends ScrollPhysics {
  const _NoImplicitHorizontalScrollPhysics({super.parent});

  @override
  _NoImplicitHorizontalScrollPhysics applyTo(ScrollPhysics? ancestor) {
    return _NoImplicitHorizontalScrollPhysics(parent: buildParent(ancestor));
  }

  @override
  bool get allowImplicitScrolling => false;
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

  final oldLineCount = _textLineCount(baseline);
  final newLineCount = _textLineCount(current);
  if (file.status == GitChangeStatus.added ||
      file.status == GitChangeStatus.untracked) {
    return _EditableDiffLineChanges(
      oldChangedLines: const <int>{},
      newChangedLines: Set<int>.from(Iterable<int>.generate(newLineCount)),
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
      if (oldLine != null && oldLine > 0 && oldLine <= oldLineCount) {
        oldChanged.add(oldLine - 1);
      }
      if (oldLine != null) oldLine += 1;
      continue;
    }
    if (line.kind == GitDiffLineKind.addition) {
      if (newLine != null && newLine > 0 && newLine <= newLineCount) {
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

int _textLineCount(String text) {
  if (text.isEmpty) return 0;
  var count = 1;
  for (var index = 0; index < text.length; index += 1) {
    if (text.codeUnitAt(index) == 0x0a) count += 1;
  }
  if (text.endsWith('\n')) count -= 1;
  return count;
}

enum _EditableLineDiffKind { equal, deletion, addition }

final class _EditableLineDiffStep {
  const _EditableLineDiffStep({
    required this.kind,
    this.oldIndex,
    this.newIndex,
  });

  final _EditableLineDiffKind kind;
  final int? oldIndex;
  final int? newIndex;
}

final class _EditableDiffSpacer {
  const _EditableDiffSpacer({required this.afterLine, required this.count});

  final int afterLine;
  final int count;
}

final class _EditableDiffAlignment {
  const _EditableDiffAlignment({
    required this.oldChangedLines,
    required this.newChangedLines,
    required this.oldSpacers,
    required this.newSpacers,
    required this.added,
    required this.removed,
    required this.visualLineCount,
  });

  final Set<int> oldChangedLines;
  final Set<int> newChangedLines;
  final List<_EditableDiffSpacer> oldSpacers;
  final List<_EditableDiffSpacer> newSpacers;
  final int added;
  final int removed;
  final int visualLineCount;

  _EditableDiffLineChanges get changes => _EditableDiffLineChanges(
    oldChangedLines: oldChangedLines,
    newChangedLines: newChangedLines,
  );
}

const int _editableDiffMaxMyersDistance = 2048;

_EditableDiffAlignment _editableDiffAlignment(
  String baseline,
  String current,
  GitDiffWhitespaceMode whitespaceMode,
) {
  final oldLines = _splitFullFileLines(baseline);
  final newLines = _splitFullFileLines(current);
  final normalizedOld = oldLines
      .map((line) => _normalizeLiveDiffLine(line, whitespaceMode))
      .toList(growable: false);
  final normalizedNew = newLines
      .map((line) => _normalizeLiveDiffLine(line, whitespaceMode))
      .toList(growable: false);
  final steps =
      _myersEditableLineDiff(normalizedOld, normalizedNew) ??
      _coarseEditableLineDiff(normalizedOld, normalizedNew);

  final oldChanged = <int>{};
  final newChanged = <int>{};
  final oldSpacers = <_EditableDiffSpacer>[];
  final newSpacers = <_EditableDiffSpacer>[];
  var oldCursor = 0;
  var newCursor = 0;
  var pendingOld = 0;
  var pendingNew = 0;
  var added = 0;
  var removed = 0;

  void flushChangeRun() {
    if (pendingOld == 0 && pendingNew == 0) return;
    added += pendingNew;
    removed += pendingOld;
    if (pendingNew > pendingOld) {
      oldSpacers.add(
        _EditableDiffSpacer(
          afterLine: oldCursor - 1,
          count: pendingNew - pendingOld,
        ),
      );
    } else if (pendingOld > pendingNew) {
      newSpacers.add(
        _EditableDiffSpacer(
          afterLine: newCursor - 1,
          count: pendingOld - pendingNew,
        ),
      );
    }
    pendingOld = 0;
    pendingNew = 0;
  }

  for (final step in steps) {
    switch (step.kind) {
      case _EditableLineDiffKind.equal:
        flushChangeRun();
        oldCursor += 1;
        newCursor += 1;
      case _EditableLineDiffKind.deletion:
        oldChanged.add(step.oldIndex!);
        oldCursor += 1;
        pendingOld += 1;
      case _EditableLineDiffKind.addition:
        newChanged.add(step.newIndex!);
        newCursor += 1;
        pendingNew += 1;
    }
  }
  flushChangeRun();

  final oldVisualCount =
      oldLines.length +
      oldSpacers.fold<int>(0, (sum, item) => sum + item.count);
  final newVisualCount =
      newLines.length +
      newSpacers.fold<int>(0, (sum, item) => sum + item.count);
  return _EditableDiffAlignment(
    oldChangedLines: oldChanged,
    newChangedLines: newChanged,
    oldSpacers: oldSpacers,
    newSpacers: newSpacers,
    added: added,
    removed: removed,
    visualLineCount: math.max(oldVisualCount, newVisualCount),
  );
}

List<_EditableLineDiffStep>? _myersEditableLineDiff(
  List<String> oldLines,
  List<String> newLines,
) {
  final oldLength = oldLines.length;
  final newLength = newLines.length;
  if (oldLength == 0) {
    return <_EditableLineDiffStep>[
      for (var index = 0; index < newLength; index += 1)
        _EditableLineDiffStep(
          kind: _EditableLineDiffKind.addition,
          newIndex: index,
        ),
    ];
  }
  if (newLength == 0) {
    return <_EditableLineDiffStep>[
      for (var index = 0; index < oldLength; index += 1)
        _EditableLineDiffStep(
          kind: _EditableLineDiffKind.deletion,
          oldIndex: index,
        ),
    ];
  }

  final maxDistance = math.min(
    oldLength + newLength,
    _editableDiffMaxMyersDistance,
  );
  var frontier = <int, int>{1: 0};
  final trace = <Map<int, int>>[];
  for (var distance = 0; distance <= maxDistance; distance += 1) {
    final next = <int, int>{};
    for (var diagonal = -distance; diagonal <= distance; diagonal += 2) {
      final down = frontier[diagonal + 1] ?? -1;
      final right = frontier[diagonal - 1] ?? -1;
      var oldIndex =
          diagonal == -distance || (diagonal != distance && right < down)
          ? math.max(0, down)
          : math.max(0, right + 1);
      var newIndex = oldIndex - diagonal;
      while (oldIndex < oldLength &&
          newIndex < newLength &&
          oldLines[oldIndex] == newLines[newIndex]) {
        oldIndex += 1;
        newIndex += 1;
      }
      next[diagonal] = oldIndex;
      if (oldIndex >= oldLength && newIndex >= newLength) {
        trace.add(next);
        return _backtrackEditableLineDiff(trace, oldLength, newLength);
      }
    }
    trace.add(next);
    frontier = next;
  }
  return null;
}

List<_EditableLineDiffStep> _backtrackEditableLineDiff(
  List<Map<int, int>> trace,
  int oldLength,
  int newLength,
) {
  final reversed = <_EditableLineDiffStep>[];
  var oldIndex = oldLength;
  var newIndex = newLength;
  for (var distance = trace.length - 1; distance > 0; distance -= 1) {
    final previous = trace[distance - 1];
    final diagonal = oldIndex - newIndex;
    final down = previous[diagonal + 1] ?? -1;
    final right = previous[diagonal - 1] ?? -1;
    final previousDiagonal =
        diagonal == -distance || (diagonal != distance && right < down)
        ? diagonal + 1
        : diagonal - 1;
    final previousOld = previous[previousDiagonal] ?? 0;
    final previousNew = previousOld - previousDiagonal;

    while (oldIndex > previousOld && newIndex > previousNew) {
      oldIndex -= 1;
      newIndex -= 1;
      reversed.add(
        _EditableLineDiffStep(
          kind: _EditableLineDiffKind.equal,
          oldIndex: oldIndex,
          newIndex: newIndex,
        ),
      );
    }
    if (oldIndex == previousOld) {
      newIndex -= 1;
      reversed.add(
        _EditableLineDiffStep(
          kind: _EditableLineDiffKind.addition,
          newIndex: newIndex,
        ),
      );
    } else {
      oldIndex -= 1;
      reversed.add(
        _EditableLineDiffStep(
          kind: _EditableLineDiffKind.deletion,
          oldIndex: oldIndex,
        ),
      );
    }
  }
  while (oldIndex > 0 && newIndex > 0) {
    oldIndex -= 1;
    newIndex -= 1;
    reversed.add(
      _EditableLineDiffStep(
        kind: _EditableLineDiffKind.equal,
        oldIndex: oldIndex,
        newIndex: newIndex,
      ),
    );
  }
  while (oldIndex > 0) {
    oldIndex -= 1;
    reversed.add(
      _EditableLineDiffStep(
        kind: _EditableLineDiffKind.deletion,
        oldIndex: oldIndex,
      ),
    );
  }
  while (newIndex > 0) {
    newIndex -= 1;
    reversed.add(
      _EditableLineDiffStep(
        kind: _EditableLineDiffKind.addition,
        newIndex: newIndex,
      ),
    );
  }
  return reversed.reversed.toList(growable: false);
}

List<_EditableLineDiffStep> _coarseEditableLineDiff(
  List<String> oldLines,
  List<String> newLines,
) {
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
  return <_EditableLineDiffStep>[
    for (var index = 0; index < prefix; index += 1)
      _EditableLineDiffStep(
        kind: _EditableLineDiffKind.equal,
        oldIndex: index,
        newIndex: index,
      ),
    for (var index = prefix; index < oldSuffix; index += 1)
      _EditableLineDiffStep(
        kind: _EditableLineDiffKind.deletion,
        oldIndex: index,
      ),
    for (var index = prefix; index < newSuffix; index += 1)
      _EditableLineDiffStep(
        kind: _EditableLineDiffKind.addition,
        newIndex: index,
      ),
    for (var offset = 0; offset < oldLines.length - oldSuffix; offset += 1)
      _EditableLineDiffStep(
        kind: _EditableLineDiffKind.equal,
        oldIndex: oldSuffix + offset,
        newIndex: newSuffix + offset,
      ),
  ];
}

List<(int, int)> _editableChangedLineRanges(Set<int> lines) {
  if (lines.isEmpty) return const <(int, int)>[];
  final sorted = lines.toList(growable: false)..sort();
  final result = <(int, int)>[];
  var start = sorted.first;
  var end = start;
  for (final line in sorted.skip(1)) {
    if (line == end + 1) {
      end = line;
      continue;
    }
    result.add((start, end));
    start = line;
    end = line;
  }
  result.add((start, end));
  return result;
}

List<({int afterLine, String content})> _editableSpacerRanges(
  List<_EditableDiffSpacer> spacers,
) => <({int afterLine, String content})>[
  for (final spacer in spacers)
    (
      afterLine: spacer.afterLine,
      content: List<String>.filled(spacer.count, '').join('\n'),
    ),
];

@visibleForTesting
({
  List<({int afterLine, int count})> oldSpacers,
  List<({int afterLine, int count})> newSpacers,
  int added,
  int removed,
  int visualLineCount,
})
workspaceEditableDiffAlignmentForTesting({
  required String baseline,
  required String current,
  GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
}) {
  final alignment = _editableDiffAlignment(baseline, current, whitespaceMode);
  return (
    oldSpacers: <({int afterLine, int count})>[
      for (final spacer in alignment.oldSpacers)
        (afterLine: spacer.afterLine, count: spacer.count),
    ],
    newSpacers: <({int afterLine, int count})>[
      for (final spacer in alignment.newSpacers)
        (afterLine: spacer.afterLine, count: spacer.count),
    ],
    added: alignment.added,
    removed: alignment.removed,
    visualLineCount: alignment.visualLineCount,
  );
}

String _widestDiffLineCandidate(String text) {
  if (text.isEmpty) return ' ';
  var bestStart = 0;
  var bestEnd = 0;
  var bestColumns = -1;
  var lineStart = 0;
  var columns = 0;

  void finishLine(int end) {
    if (columns > bestColumns) {
      bestColumns = columns;
      bestStart = lineStart;
      bestEnd = end;
    }
  }

  for (var index = 0; index < text.length; index += 1) {
    final unit = text.codeUnitAt(index);
    if (unit == 0x0a || unit == 0x0d) {
      finishLine(index);
      if (unit == 0x0d &&
          index + 1 < text.length &&
          text.codeUnitAt(index + 1) == 0x0a) {
        index += 1;
      }
      lineStart = index + 1;
      columns = 0;
      continue;
    }
    columns += switch (unit) {
      0x09 => 4,
      <= 0x7f => 1,
      _ => 2,
    };
  }
  finishLine(text.length);
  if (bestEnd <= bestStart) return ' ';
  return text.substring(bestStart, bestEnd);
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
    // The editable diff owns the TextField and already rebuilds itself on
    // input. Mutating the document here is enough for save/dirty state; a
    // parent surface setState would rebuild the entire diff tree for every
    // keystroke and is especially expensive for full-file side-by-side views.
    document.currentText = text;
    document.error = null;
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

/// About half a second at 60 Hz, matching the editor tab's restore window.
const int _editableDiffScrollRestoreAttempts = 30;

class _EditableWorkingTreeDiff extends StatefulWidget {
  const _EditableWorkingTreeDiff({
    required this.file,
    required this.syntax,
    required this.baseline,
    required this.document,
    required this.whitespaceMode,
    this.viewportHeight,
    required this.onChanged,
    required this.onSave,
    this.scrollStorageKey,
  });

  final GitDiffFile file;
  final _DiffSyntaxStyle syntax;
  final String baseline;
  final _EditableWorkingTreeDocument document;
  final GitDiffWhitespaceMode whitespaceMode;
  final double? viewportHeight;
  final ValueChanged<String> onChanged;
  final VoidCallback onSave;

  /// Where this file's pane offsets live in the route's [PageStorage]. The
  /// panes are CodeForge editors with their own controllers, so they are saved
  /// and restored explicitly rather than through a [PageStorageKey].
  final String? scrollStorageKey;

  @override
  State<_EditableWorkingTreeDiff> createState() =>
      _EditableWorkingTreeDiffState();
}

class _DiffOverviewRuler extends StatelessWidget {
  const _DiffOverviewRuler({
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
      message: context.tr('Diff Overview'),
      child: Semantics(
        button: true,
        label: context.tr(
          'Diff overview, ${changes.oldChangedLines.length} removed, '
          '${changes.newChangedLines.length} added',
        ),
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
                        if (scrollController.hasClients &&
                            scrollController.position.hasContentDimensions) {
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
                          painter: _DiffOverviewPainter(
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

class _DiffOverviewPainter extends CustomPainter {
  const _DiffOverviewPainter({
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
  bool shouldRepaint(covariant _DiffOverviewPainter oldDelegate) =>
      oldDelegate.oldChangedLines != oldChangedLines ||
      oldDelegate.newChangedLines != newChangedLines ||
      oldDelegate.lineCount != lineCount ||
      oldDelegate.viewportStart != viewportStart ||
      oldDelegate.viewportEnd != viewportEnd;
}

class _EditableWorkingTreeDiffState extends State<_EditableWorkingTreeDiff> {
  code_forge.CodeForgeController? _leftController;
  code_forge.CodeForgeController? _controller;
  code_forge.UndoRedoController? _leftUndoController;
  code_forge.UndoRedoController? _undoController;
  late final _DiffSyntaxTextEditingController _leftFallbackController;
  late final _DiffSyntaxTextEditingController _fallbackController;
  late final FocusNode _leftFocusNode;
  late final FocusNode _rightFocusNode;
  late final ScrollController _leftHorizontalController;
  late final ScrollController _rightHorizontalController;
  late final ScrollController _leftVerticalController;
  late final ScrollController _rightVerticalController;
  late _EditableDiffAlignment _alignment;
  var _syncingHorizontal = false;
  var _syncingVertical = false;
  var _syncingControllerText = false;
  var _restoredScroll = false;

  @override
  void initState() {
    super.initState();
    _leftFallbackController = _DiffSyntaxTextEditingController(
      text: widget.baseline,
      syntax: widget.syntax,
    );
    _fallbackController = _DiffSyntaxTextEditingController(
      text: widget.document.currentText,
      syntax: widget.syntax,
    );
    try {
      _leftController = code_forge.CodeForgeController()
        ..text = widget.baseline;
      _controller = code_forge.CodeForgeController()
        ..text = widget.document.currentText;
      _leftUndoController = code_forge.UndoRedoController();
      _undoController = code_forge.UndoRedoController();
    } on StateError {
      _leftController = null;
      _controller = null;
      _leftUndoController = null;
      _undoController = null;
    }
    _leftFocusNode = FocusNode();
    _rightFocusNode = FocusNode();
    _leftHorizontalController = ScrollController();
    _rightHorizontalController = ScrollController();
    _leftVerticalController = ScrollController();
    _rightVerticalController = ScrollController();
    _alignment = _editableDiffAlignment(
      widget.baseline,
      widget.document.currentText,
      widget.whitespaceMode,
    );
    _controller?.addListener(_handleEditableControllerChanged);
    _leftHorizontalController.addListener(_syncHorizontalFromLeft);
    _rightHorizontalController.addListener(_syncHorizontalFromRight);
    _leftVerticalController.addListener(_syncVerticalFromLeft);
    _rightVerticalController.addListener(_syncVerticalFromRight);
    _applyAlignmentDecorations();
  }

  @override
  void didUpdateWidget(covariant _EditableWorkingTreeDiff oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.syntax, widget.syntax)) {
      _leftFallbackController.updateSyntax(widget.syntax);
      _fallbackController.updateSyntax(widget.syntax);
    }
    if (oldWidget.baseline != widget.baseline &&
        _leftDisplayedText != widget.baseline) {
      _replaceDisplayedText(isLeft: true, text: widget.baseline);
    }
    if (_editableDisplayedText != widget.document.currentText &&
        oldWidget.document.currentText != widget.document.currentText) {
      _replaceDisplayedText(isLeft: false, text: widget.document.currentText);
    }
    if (oldWidget.baseline != widget.baseline ||
        oldWidget.document.currentText != widget.document.currentText ||
        oldWidget.whitespaceMode != widget.whitespaceMode) {
      _refreshAlignment(notify: false);
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_restoredScroll) return;
    _restoredScroll = true;
    final key = widget.scrollStorageKey;
    final saved = key == null
        ? null
        : PageStorage.maybeOf(context)?.readState(context, identifier: key);
    if (saved is List<double?>) {
      _scheduleScrollRestore(saved, attempt: 0);
    }
  }

  @override
  void deactivate() {
    // Before children unmount: dispose() would find the panes detached.
    final key = widget.scrollStorageKey;
    if (key != null) {
      PageStorage.maybeOf(context)?.writeState(context, <double?>[
        for (final controller in _paneControllers)
          controller.hasClients ? controller.offset : null,
      ], identifier: key);
    }
    super.deactivate();
  }

  List<ScrollController> get _paneControllers => <ScrollController>[
    _leftVerticalController,
    _rightVerticalController,
    _leftHorizontalController,
    _rightHorizontalController,
  ];

  void _scheduleScrollRestore(List<double?> offsets, {required int attempt}) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      var done = true;
      final controllers = _paneControllers;
      for (var i = 0; i < controllers.length && i < offsets.length; i++) {
        final offset = offsets[i];
        final controller = controllers[i];
        if (offset == null) continue;
        if (!controller.hasClients ||
            !controller.position.hasContentDimensions) {
          done = false;
          continue;
        }
        final position = controller.position;
        final target = offset
            .clamp(position.minScrollExtent, position.maxScrollExtent)
            .toDouble();
        if (position.pixels != target) controller.jumpTo(target);
        if (target != offset) done = false;
      }
      // The editors lay a long file out over several frames; until then the
      // extent is short and the offset would be clamped short.
      if (!done && attempt < _editableDiffScrollRestoreAttempts) {
        _scheduleScrollRestore(offsets, attempt: attempt + 1);
        WidgetsBinding.instance.scheduleFrame();
      }
    });
  }

  @override
  void dispose() {
    _controller?.removeListener(_handleEditableControllerChanged);
    _leftHorizontalController.dispose();
    _rightHorizontalController.dispose();
    _leftVerticalController.dispose();
    _rightVerticalController.dispose();
    _leftFocusNode.dispose();
    _rightFocusNode.dispose();
    _leftUndoController?.dispose();
    _undoController?.dispose();
    _leftController?.dispose();
    _controller?.dispose();
    _leftFallbackController.dispose();
    _fallbackController.dispose();
    super.dispose();
  }

  bool get _usesCodeForge => _controller != null && _leftController != null;
  String get _leftDisplayedText =>
      _leftController?.text ?? _leftFallbackController.text;
  String get _editableDisplayedText =>
      _controller?.text ?? _fallbackController.text;

  void _replaceDisplayedText({required bool isLeft, required String text}) {
    _syncingControllerText = true;
    try {
      if (isLeft) {
        final controller = _leftController;
        if (controller != null) {
          controller.text = text;
        }
        if (_leftFallbackController.text != text) {
          _leftFallbackController.text = text;
        }
      } else {
        final controller = _controller;
        if (controller != null) {
          controller.text = text;
        }
        if (_fallbackController.text != text) {
          _fallbackController.text = text;
        }
      }
    } finally {
      _syncingControllerText = false;
    }
  }

  void _handleEditableControllerChanged() {
    if (_syncingControllerText) return;
    final text = _controller?.text ?? _fallbackController.text;
    if (text == widget.document.currentText) return;
    if (_fallbackController.text != text) {
      _fallbackController.text = text;
    }
    widget.onChanged(text);
    _refreshAlignment();
  }

  void _handleFallbackChanged(String text) {
    if (_syncingControllerText || text == widget.document.currentText) return;
    widget.onChanged(text);
    _refreshAlignment();
  }

  void _refreshAlignment({bool notify = true}) {
    _alignment = _editableDiffAlignment(
      widget.baseline,
      _editableDisplayedText,
      widget.whitespaceMode,
    );
    _applyAlignmentDecorations();
    if (notify && mounted) setState(() {});
  }

  void _applyAlignmentDecorations() {
    if (!_usesCodeForge) return;
    const spacerColor = Colors.transparent;
    _leftController!.setGitDiffDecorations(
      addedRanges: _editableChangedLineRanges(_alignment.oldChangedLines),
      removedRanges: _editableSpacerRanges(_alignment.oldSpacers),
      addedColor: AleraTokens.error,
      removedColor: spacerColor,
    );
    _controller!.setGitDiffDecorations(
      addedRanges: _editableChangedLineRanges(_alignment.newChangedLines),
      removedRanges: _editableSpacerRanges(_alignment.newSpacers),
      addedColor: AleraTokens.success,
      removedColor: spacerColor,
    );
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
      painter.text = TextSpan(
        text: _widestDiffLineCandidate(content),
        style: textStyle,
      );
      painter.layout();
      maxWidth = math.max(maxWidth, painter.width);
    }
    return maxWidth * 1.08 + AleraTokens.space24;
  }

  @override
  Widget build(BuildContext context) {
    final stats = _LiveDiffStats(
      added: _alignment.added,
      removed: _alignment.removed,
    );
    const lineHeight = 18.0;
    final fallbackHeight = (_alignment.visualLineCount * lineHeight + 72).clamp(
      260.0,
      620.0,
    );
    final viewportHeight = widget.viewportHeight;
    final height =
        viewportHeight != null && viewportHeight.isFinite && viewportHeight > 0
        ? viewportHeight
        : fallbackHeight;
    final textStyle = widget.syntax.textStyle;

    Widget buildPane({required bool isLeft}) {
      final horizontalController = isLeft
          ? _leftHorizontalController
          : _rightHorizontalController;
      final verticalController = isLeft
          ? _leftVerticalController
          : _rightVerticalController;
      final focusNode = isLeft ? _leftFocusNode : _rightFocusNode;
      final side = isLeft ? 'original' : 'editor';
      if (!_usesCodeForge) {
        final fallbackController = isLeft
            ? _leftFallbackController
            : _fallbackController;
        return LayoutBuilder(
          builder: (context, constraints) {
            final contentWidth = math.max(
              constraints.maxWidth,
              _sharedContentWidth(context, textStyle),
            );
            return Scrollbar(
              key: ValueKey<String>(
                'git-diff-working-tree-$side-x-scrollbar-${widget.file.path}',
              ),
              controller: horizontalController,
              thumbVisibility: true,
              scrollbarOrientation: ScrollbarOrientation.bottom,
              notificationPredicate: (notification) =>
                  notification.metrics.axis == Axis.horizontal,
              child: SingleChildScrollView(
                controller: horizontalController,
                scrollDirection: Axis.horizontal,
                physics: const _NoImplicitHorizontalScrollPhysics(),
                child: SizedBox(
                  width: contentWidth,
                  height: constraints.maxHeight,
                  child: Scrollbar(
                    key: ValueKey<String>(
                      'git-diff-working-tree-$side-y-scrollbar-${widget.file.path}',
                    ),
                    controller: verticalController,
                    thumbVisibility: true,
                    notificationPredicate: (notification) =>
                        notification.metrics.axis == Axis.vertical,
                    child: TextField(
                      key: ValueKey<String>(
                        'git-diff-working-tree-$side-${widget.file.path}',
                      ),
                      controller: fallbackController,
                      focusNode: focusNode,
                      scrollController: verticalController,
                      expands: true,
                      maxLines: null,
                      minLines: null,
                      readOnly: isLeft,
                      enableInteractiveSelection: true,
                      keyboardType: TextInputType.multiline,
                      style: textStyle,
                      decoration: const InputDecoration(
                        border: InputBorder.none,
                        filled: false,
                        contentPadding: EdgeInsets.all(AleraTokens.space8),
                      ),
                      onChanged: isLeft ? null : _handleFallbackChanged,
                    ),
                  ),
                ),
              ),
            );
          },
        );
      }
      final controller = isLeft ? _leftController! : _controller!;
      final undoController = isLeft ? _leftUndoController! : _undoController!;
      return Scrollbar(
        key: ValueKey<String>(
          'git-diff-working-tree-$side-x-scrollbar-${widget.file.path}',
        ),
        controller: horizontalController,
        thumbVisibility: true,
        scrollbarOrientation: ScrollbarOrientation.bottom,
        notificationPredicate: (notification) =>
            notification.metrics.axis == Axis.horizontal,
        child: Scrollbar(
          key: ValueKey<String>(
            'git-diff-working-tree-$side-y-scrollbar-${widget.file.path}',
          ),
          controller: verticalController,
          thumbVisibility: true,
          notificationPredicate: (notification) =>
              notification.metrics.axis == Axis.vertical,
          child: code_forge.CodeForge(
            key: ValueKey<String>(
              'git-diff-working-tree-$side-${widget.file.path}',
            ),
            controller: controller,
            undoController: undoController,
            verticalScrollController: verticalController,
            horizontalScrollController: horizontalController,
            focusNode: focusNode,
            readOnly: isLeft,
            autoFocus: false,
            lineWrap: false,
            enableFolding: false,
            enableGuideLines: false,
            enableGutter: true,
            enableGutterDivider: false,
            enableLocalSuggestions: false,
            enableNativeSyntax: false,
            language: widget.syntax.language,
            languageId: widget.syntax.languageId,
            editorTheme: widget.syntax.editorTheme,
            textStyle: textStyle,
            innerPadding: const EdgeInsets.all(AleraTokens.space8),
          ),
        ),
      );
    }

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
                    child: DecoratedBox(
                      decoration: const BoxDecoration(
                        border: Border(
                          right: BorderSide(color: AleraTokens.borderSubtle),
                        ),
                      ),
                      child: buildPane(isLeft: true),
                    ),
                  ),
                  Expanded(child: buildPane(isLeft: false)),
                  SizedBox(
                    width: _DiffOverviewRuler.width,
                    child: _DiffOverviewRuler(
                      key: ValueKey<String>(
                        'git-diff-working-tree-overview-${widget.file.path}',
                      ),
                      changes: _alignment.changes,
                      lineCount: math.max(_alignment.visualLineCount, 1),
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
