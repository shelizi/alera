part of 'workspace_git_diff_surface.dart';

final class _SingleColumnDiffOverviewProjection {
  const _SingleColumnDiffOverviewProjection({
    required this.changes,
    required this.lineCount,
  });

  final _EditableDiffLineChanges changes;
  final int lineCount;
}

_SingleColumnDiffOverviewProjection _singleColumnDiffOverviewProjection({
  required GitDiffResult result,
  required Map<GitDiffFile, _FullFileContents> fullFileContents,
  required Set<String> fullFilePreviewLimitedPaths,
  required GitDiffContentMode contentMode,
}) {
  final oldChangedLines = <int>{};
  final newChangedLines = <int>{};
  var globalOffset = 0;

  for (final file in result.files) {
    if (file.isBinary || file.isLarge || file.isGitlink) {
      globalOffset += 1;
      continue;
    }
    final fileContentMode =
        contentMode == GitDiffContentMode.fullFile &&
            fullFilePreviewLimitedPaths.contains(file.path)
        ? GitDiffContentMode.diffOnly
        : contentMode;
    final contents = fullFileContents[file];
    var oldLineCount = 0;
    var newLineCount = 0;
    if (fileContentMode == GitDiffContentMode.fullFile) {
      oldLineCount = _textLineCount(contents?.oldDecoded?.content ?? '');
      newLineCount = _textLineCount(contents?.newDecoded?.content ?? '');
    }
    var estimatedLineCount = math.max(oldLineCount, newLineCount);
    int? oldLine;
    int? newLine;

    for (final line in file.lines) {
      if (line.kind == GitDiffLineKind.hunk) {
        final match = _hunkHeaderRegExp.firstMatch(line.text);
        if (match == null) continue;
        final oldStart = int.tryParse(match.group(1) ?? '');
        final newStart = int.tryParse(match.group(3) ?? '');
        final oldCount = int.tryParse(match.group(2) ?? '') ?? 1;
        final newCount = int.tryParse(match.group(4) ?? '') ?? 1;
        oldLine = oldCount == 0 ? null : oldStart;
        newLine = newCount == 0 ? null : newStart;
        if (oldStart != null) {
          final oldSpan = oldCount > 1 ? oldCount : 1;
          estimatedLineCount = math.max(
            estimatedLineCount,
            oldStart + oldSpan - 1,
          );
        }
        if (newStart != null) {
          final newSpan = newCount > 1 ? newCount : 1;
          estimatedLineCount = math.max(
            estimatedLineCount,
            newStart + newSpan - 1,
          );
        }
        continue;
      }
      if (line.kind == GitDiffLineKind.header) continue;
      if (line.kind == GitDiffLineKind.deletion) {
        if (oldLine != null && oldLine > 0) {
          oldChangedLines.add(globalOffset + oldLine - 1);
          estimatedLineCount = math.max(estimatedLineCount, oldLine);
          oldLine += 1;
        }
        continue;
      }
      if (line.kind == GitDiffLineKind.addition) {
        if (newLine != null && newLine > 0) {
          newChangedLines.add(globalOffset + newLine - 1);
          estimatedLineCount = math.max(estimatedLineCount, newLine);
          newLine += 1;
        }
        continue;
      }
      if (line.kind == GitDiffLineKind.context) {
        if (oldLine != null) {
          estimatedLineCount = math.max(estimatedLineCount, oldLine);
          oldLine += 1;
        }
        if (newLine != null) {
          estimatedLineCount = math.max(estimatedLineCount, newLine);
          newLine += 1;
        }
      }
    }

    // Keep each file represented in the global overview even when the patch
    // has no parseable hunk header. One separator line keeps adjacent file
    // markers from collapsing onto the same ruler position.
    globalOffset += math.max(estimatedLineCount, 1) + 1;
  }

  return _SingleColumnDiffOverviewProjection(
    changes: _EditableDiffLineChanges(
      oldChangedLines: oldChangedLines,
      newChangedLines: newChangedLines,
    ),
    lineCount: math.max(globalOffset - 1, 1),
  );
}

class _SingleColumnDiffList extends StatefulWidget {
  const _SingleColumnDiffList({
    required this.rows,
    required this.overview,
    required this.selectionEnabled,
  });

  final _DiffRows rows;
  final _SingleColumnDiffOverviewProjection overview;
  final bool selectionEnabled;

  @override
  State<_SingleColumnDiffList> createState() => _SingleColumnDiffListState();
}

class _SingleColumnDiffListState extends State<_SingleColumnDiffList> {
  late final ScrollController _scrollController;

  @override
  void initState() {
    super.initState();
    _scrollController = ScrollController();
  }

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final list = ListView.builder(
      key: const ValueKey<String>('git-diff-single-column-list'),
      controller: _scrollController,
      padding: const EdgeInsets.only(bottom: AleraTokens.space16),
      itemCount: widget.rows.length,
      itemBuilder: (context, index) => widget.rows.rowAt(index).build(context),
    );
    return Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Expanded(
          child: widget.selectionEnabled ? SelectionArea(child: list) : list,
        ),
        SizedBox(
          width: _DiffOverviewRuler.width,
          child: _DiffOverviewRuler(
            key: const ValueKey<String>('git-diff-single-column-overview'),
            changes: widget.overview.changes,
            lineCount: widget.overview.lineCount,
            scrollController: _scrollController,
          ),
        ),
      ],
    );
  }
}

enum _DiffOnlySideBySideSelectionPane { left, right }

class _DiffOnlySideBySideSelectionScope extends InheritedWidget {
  const _DiffOnlySideBySideSelectionScope({
    required this.activePane,
    required super.child,
  });

  final _DiffOnlySideBySideSelectionPane? activePane;

  static _DiffOnlySideBySideSelectionPane? activePaneOf(
    BuildContext context,
  ) => context
      .dependOnInheritedWidgetOfExactType<_DiffOnlySideBySideSelectionScope>()
      ?.activePane;

  @override
  bool updateShouldNotify(_DiffOnlySideBySideSelectionScope oldWidget) =>
      activePane != oldWidget.activePane;
}

class _DiffOnlySideBySideSelectionArea extends StatefulWidget {
  const _DiffOnlySideBySideSelectionArea({
    required this.contentWidth,
    required this.child,
  });

  final double contentWidth;
  final Widget child;

  @override
  State<_DiffOnlySideBySideSelectionArea> createState() =>
      _DiffOnlySideBySideSelectionAreaState();
}

class _DiffOnlySideBySideSelectionAreaState
    extends State<_DiffOnlySideBySideSelectionArea> {
  _DiffOnlySideBySideSelectionPane? _activePane;

  void _handlePointerDown(PointerDownEvent event) {
    final next = event.localPosition.dx < widget.contentWidth / 2
        ? _DiffOnlySideBySideSelectionPane.left
        : _DiffOnlySideBySideSelectionPane.right;
    if (_activePane == next) return;
    setState(() => _activePane = next);
  }

  @override
  Widget build(BuildContext context) {
    return Listener(
      behavior: HitTestBehavior.translucent,
      onPointerDown: _handlePointerDown,
      child: _DiffOnlySideBySideSelectionScope(
        activePane: _activePane,
        child: SelectionArea(child: widget.child),
      ),
    );
  }
}

class const _DiffFileList({
  required final GitDiffResult result,
  required final Map<GitDiffFile, _FullFileContents> fullFileContents,
  final Set<String> fullFilePreviewLimitedPaths = const <String>{},
  required final String sourcePath,
  final String? sourceLabel,
  final String? commitOid,
  final String? parentOid,
  final GitDiffContentMode contentMode = GitDiffContentMode.fullFile,
  final GitDiffPresentationMode presentationMode =
      GitDiffPresentationMode.unified,
  final GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
  final Map<String, _EditableWorkingTreeDocument> editableDocuments = const {},
  final void Function(GitDiffFile file, String text)? onEditableChanged,
  final void Function(GitDiffFile file)? onEditableSave,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final hasEditableSideBySide =
        presentationMode == GitDiffPresentationMode.sideBySide &&
        contentMode == GitDiffContentMode.fullFile &&
        onEditableChanged != null &&
        onEditableSave != null &&
        result.files.any(
          (file) =>
              !file.isBinary &&
              !file.isLarge &&
              !file.isGitlink &&
              !fullFilePreviewLimitedPaths.contains(file.path) &&
              editableDocuments.containsKey(file.path),
        );

    _DiffRows buildRows({double? editableViewportHeight}) {
      return _DiffRows.fromResult(
        result,
        fullFileContents: fullFileContents,
        fullFilePreviewLimitedPaths: fullFilePreviewLimitedPaths,
        sourcePath: sourcePath,
        sourceLabel: sourceLabel,
        commitOid: commitOid,
        parentOid: parentOid,
        contentMode: contentMode,
        presentationMode: presentationMode,
        whitespaceMode: whitespaceMode,
        editableDocuments: editableDocuments,
        onEditableChanged: onEditableChanged,
        onEditableSave: onEditableSave,
        editableViewportHeight: editableViewportHeight,
      );
    }

    if (presentationMode == GitDiffPresentationMode.sideBySide) {
      return LayoutBuilder(
        builder: (context, constraints) {
          final contentWidth = math.max(constraints.maxWidth, 720.0);
          final rows = buildRows(
            editableViewportHeight: constraints.hasBoundedHeight
                ? constraints.maxHeight
                : null,
          );
          final list = ListView.builder(
            padding: const EdgeInsets.only(bottom: AleraTokens.space16),
            itemCount: rows.length,
            itemBuilder: (context, index) => rows.rowAt(index).build(context),
          );
          final selectableList =
              !hasEditableSideBySide &&
                  contentMode == GitDiffContentMode.diffOnly
              ? _DiffOnlySideBySideSelectionArea(
                  contentWidth: contentWidth,
                  child: list,
                )
              : list;
          final sideBySideView = SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: SizedBox(
              width: contentWidth,
              height: constraints.maxHeight,
              child: selectableList,
            ),
          );
          return hasEditableSideBySide ||
                  contentMode == GitDiffContentMode.diffOnly
              ? sideBySideView
              : SelectionArea(child: sideBySideView);
        },
      );
    }
    final rows = buildRows();
    return _SingleColumnDiffList(
      rows: rows,
      selectionEnabled: true,
      overview: _singleColumnDiffOverviewProjection(
        result: result,
        fullFileContents: fullFileContents,
        fullFilePreviewLimitedPaths: fullFilePreviewLimitedPaths,
        contentMode: contentMode,
      ),
    );
  }
}

abstract class _DiffRowSpan {
  int get length;

  _DiffRow rowAt(int index);
}

class _MaterializedDiffRowSpan implements _DiffRowSpan {
  const _MaterializedDiffRowSpan(this.rows);

  final List<_DiffRow> rows;

  @override
  int get length => rows.length;

  @override
  _DiffRow rowAt(int index) => rows[index];
}

class _DiffLineRowSpan implements _DiffRowSpan {
  const _DiffLineRowSpan(this.lines);

  final List<GitDiffLine> lines;

  @override
  int get length => lines.length;

  @override
  _DiffRow rowAt(int index) => _DiffLineRow(lines[index]);
}

class _DiffRowsBuilder {
  final List<_DiffRowSpan> _spans = <_DiffRowSpan>[];
  final List<_DiffRow> _pending = <_DiffRow>[];

  void add(_DiffRow row) => _pending.add(row);

  void addAll(Iterable<_DiffRow> rows) => _pending.addAll(rows);

  void addSpans(Iterable<_DiffRowSpan> spans) {
    _flushPending();
    _spans.addAll(spans.where((span) => span.length > 0));
  }

  void _flushPending() {
    if (_pending.isEmpty) return;
    _spans.add(_MaterializedDiffRowSpan(List<_DiffRow>.unmodifiable(_pending)));
    _pending.clear();
  }

  _DiffRows build() {
    _flushPending();
    return _DiffRows(_spans);
  }
}

class _DiffRows {
  _DiffRows(List<_DiffRowSpan> spans)
    : _spans = List<_DiffRowSpan>.unmodifiable(spans) {
    var total = 0;
    _spanEnds = <int>[];
    for (final span in _spans) {
      total += span.length;
      _spanEnds.add(total);
    }
    length = total;
  }

  final List<_DiffRowSpan> _spans;
  late final List<int> _spanEnds;
  late final int length;

  _DiffRow rowAt(int index) {
    if (index < 0 || index >= length) {
      throw RangeError.index(index, this, 'index', null, length);
    }
    var low = 0;
    var high = _spanEnds.length - 1;
    while (low < high) {
      final mid = (low + high) >> 1;
      if (index < _spanEnds[mid]) {
        high = mid;
      } else {
        low = mid + 1;
      }
    }
    final start = low == 0 ? 0 : _spanEnds[low - 1];
    return _spans[low].rowAt(index - start);
  }

  factory _DiffRows.fromResult(
    GitDiffResult result, {
    required Map<GitDiffFile, _FullFileContents> fullFileContents,
    Set<String> fullFilePreviewLimitedPaths = const <String>{},
    required String sourcePath,
    String? sourceLabel,
    String? commitOid,
    String? parentOid,
    GitDiffContentMode contentMode = GitDiffContentMode.fullFile,
    GitDiffPresentationMode presentationMode = GitDiffPresentationMode.unified,
    GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
    Map<String, _EditableWorkingTreeDocument> editableDocuments = const {},
    void Function(GitDiffFile file, String text)? onEditableChanged,
    void Function(GitDiffFile file)? onEditableSave,
    double? editableViewportHeight,
  }) {
    final items = _DiffRowsBuilder();
    if (result.truncated) {
      items.add(const _BannerRow('Diff truncated for preview.'));
    }
    for (final file in result.files) {
      items.add(_FileHeaderRow(file, sourceLabel: sourceLabel));
      if (file.isBinary && isWorkspaceImageFilePath(file.path)) {
        items.add(
          _ImageDiffRow(
            file: file,
            sourcePath: sourcePath,
            commitOid: commitOid,
            parentOid: parentOid,
          ),
        );
      } else if (file.isBinary) {
        items.add(const _BannerRow('Binary file diff is not shown.'));
      } else if (file.isLarge) {
        items.add(const _BannerRow('Large untracked file diff is not shown.'));
      } else {
        final sideBySide =
            presentationMode == GitDiffPresentationMode.sideBySide;
        final fileContentMode =
            contentMode == GitDiffContentMode.fullFile &&
                fullFilePreviewLimitedPaths.contains(file.path)
            ? GitDiffContentMode.diffOnly
            : contentMode;
        if (fileContentMode != contentMode) {
          items.add(
            const _BannerRow(
              'Full file preview is limited for large files. Showing diff only.',
            ),
          );
        }
        final editable = editableDocuments[file.path];
        if (sideBySide &&
            fileContentMode == GitDiffContentMode.fullFile &&
            editable != null &&
            onEditableChanged != null &&
            onEditableSave != null) {
          final contents = fullFileContents[file];
          items.add(
            _WidgetDiffRow(
              _EditableWorkingTreeDiff(
                file: file,
                baseline: contents?.oldDecoded?.content ?? '',
                document: editable,
                whitespaceMode: whitespaceMode,
                viewportHeight: editableViewportHeight,
                onChanged: (text) => onEditableChanged(file, text),
                onSave: () => onEditableSave(file),
              ),
            ),
          );
          continue;
        }
        List<_DiffRow>? renderedRows;
        var renderedLazyRows = false;
        if (fileContentMode == GitDiffContentMode.fullFile) {
          final contents = fullFileContents[file];
          final decodedFile = _fileWithDecodedDiffLines(file, contents);
          if (sideBySide) {
            final projectedSpans = _buildProjectedFullFileSideBySideRowSpans(
              decodedFile,
              contents,
            );
            if (projectedSpans != null) {
              items.addSpans(projectedSpans);
              renderedLazyRows = true;
            } else {
              renderedRows = _buildFullFileSideBySideRows(
                decodedFile,
                contents,
              );
            }
          } else {
            final projectedSpans = _buildProjectedFullFileRowSpans(
              decodedFile,
              contents?.singleSideDecoded(file),
            );
            if (projectedSpans != null) {
              items.addSpans(projectedSpans);
              renderedLazyRows = true;
            } else {
              renderedRows = _buildFullFileRows(
                decodedFile,
                contents?.singleSideDecoded(file),
              );
            }
          }
        }
        if (renderedLazyRows) {
          // Native full-file projections keep unchanged ranges lazy in both
          // presentation modes until the ListView requests a concrete row.
        } else if (renderedRows != null) {
          items.addAll(renderedRows);
        } else if (file.lines.isEmpty) {
          items.add(const _BannerRow('No text diff for this file.'));
        } else if (sideBySide) {
          final decodedFile = _fileWithDecodedDiffLines(
            file,
            fullFileContents[file],
          );
          final projectedSpans = _buildProjectedSideBySideRowSpans(decodedFile);
          if (projectedSpans != null) {
            items.addSpans(projectedSpans);
          } else {
            items.addAll(_buildSideBySideRows(decodedFile));
          }
        } else {
          final lines = _decodedDiffLines(file, fullFileContents[file]);
          items.addSpans(<_DiffRowSpan>[
            if (lines.isNotEmpty) _DiffLineRowSpan(lines),
          ]);
        }
        if (file.linePreviewTruncated) {
          items.add(const _BannerRow('Diff line preview truncated.'));
        }
      }
      if (file.truncated) {
        items.add(const _BannerRow('File diff truncated for preview.'));
      }
    }
    return items.build();
  }
}

abstract class const _DiffRow() {
  Widget build(BuildContext context);
}

class const _WidgetDiffRow(final Widget child) extends _DiffRow {
  @override
  Widget build(BuildContext context) => child;
}

class const _FileHeaderRow(final GitDiffFile file, {final String? sourceLabel})
    extends _DiffRow {
  @override
  Widget build(BuildContext context) {
    return SelectionContainer.disabled(
      child: DecoratedBox(
        decoration: const BoxDecoration(
          color: AleraTokens.surfaceVariant,
          border: Border(bottom: BorderSide(color: AleraTokens.borderSubtle)),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: AleraTokens.space12,
            vertical: AleraTokens.space8,
          ),
          child: Row(
            children: <Widget>[
              AleraFileIcon(pathOrName: file.path, kind: .file, size: 16),
              const SizedBox(width: AleraTokens.space8),
              Expanded(
                child: Text(
                  '${sourceLabel ?? file.sourceLabel ?? file.area.label} · ${file.path}',
                  maxLines: 1,
                  overflow: .ellipsis,
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: AleraTokens.foreground,
                    fontFamily: 'JetBrains Mono',
                  ),
                ),
              ),
              _DiffStats(file: file),
            ],
          ),
        ),
      ),
    );
  }
}

class const _BannerRow(final String message) extends _DiffRow {
  @override
  Widget build(BuildContext context) => _DiffBanner(message: message);
}

class const _ImageDiffRow({
  required final GitDiffFile file,
  required final String sourcePath,
  final String? commitOid,
  final String? parentOid,
}) extends _DiffRow {
  @override
  Widget build(BuildContext context) => WorkspaceGitDiffImageRow(
    file: file,
    sourcePath: sourcePath,
    commitOid: commitOid,
    parentOid: parentOid,
  );
}

class const _DiffLineRow(final GitDiffLine text) extends _DiffRow {
  @override
  Widget build(BuildContext context) => _DiffLine(line: text);
}

class const _DiffStats({required final GitDiffFile file})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final visibleAdded = file.added != null && file.added! > 0
        ? file.added
        : null;
    final visibleRemoved = file.removed != null && file.removed! > 0
        ? file.removed
        : null;
    final style = Theme.of(context).textTheme.labelSmall;
    return Row(
      mainAxisSize: .min,
      children: <Widget>[
        if (visibleAdded case final added?)
          Text('+$added', style: style?.copyWith(color: AleraTokens.success)),
        if (visibleRemoved case final removed?) ...<Widget>[
          const SizedBox(width: AleraTokens.space6),
          Text('-$removed', style: style?.copyWith(color: AleraTokens.error)),
        ],
      ],
    );
  }
}

class const _DiffLine({required final GitDiffLine line})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final (color, background) = switch (line.kind) {
      GitDiffLineKind.addition => (
        AleraTokens.success,
        AleraTokens.success.withValues(alpha: 0.08),
      ),
      GitDiffLineKind.deletion => (
        AleraTokens.error,
        AleraTokens.error.withValues(alpha: 0.08),
      ),
      GitDiffLineKind.hunk => (AleraTokens.warning, AleraTokens.surfaceVariant),
      GitDiffLineKind.header || GitDiffLineKind.context => (
        AleraTokens.foregroundMuted,
        Colors.transparent,
      ),
    };
    return DecoratedBox(
      decoration: BoxDecoration(color: background),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: AleraTokens.space12,
          vertical: AleraTokens.space2,
        ),
        child: Text(
          line.text,
          maxLines: 1,
          overflow: .visible,
          softWrap: false,
          style: AleraTokens.monoStyle.copyWith(fontSize: 12, color: color),
        ),
      ),
    );
  }
}

class const _DiffBanner({required final String message})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(AleraTokens.space12),
      child: Text(
        message,
        style: Theme.of(context).textTheme.bodySmall
            ?.copyWith(color: AleraTokens.foregroundMuted),
      ),
    );
  }
}

class const _DiffMessage({required final String message})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Center(
      child: Text(
        message,
        style: Theme.of(context).textTheme.bodySmall
            ?.copyWith(color: AleraTokens.foregroundMuted),
      ),
    );
  }
}
