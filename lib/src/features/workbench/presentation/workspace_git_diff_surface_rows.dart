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

int _maxDiffDisplayColumns(String text) {
  var maxColumns = 0;
  var columns = 0;
  for (var index = 0; index < text.length; index += 1) {
    final unit = text.codeUnitAt(index);
    if (unit == 0x0a || unit == 0x0d) {
      maxColumns = math.max(maxColumns, columns);
      if (unit == 0x0d &&
          index + 1 < text.length &&
          text.codeUnitAt(index + 1) == 0x0a) {
        index += 1;
      }
      columns = 0;
      continue;
    }
    columns += switch (unit) {
      0x09 => 4,
      <= 0x7f => 1,
      _ => 2,
    };
  }
  return math.max(maxColumns, columns);
}

double _readOnlyDiffContentWidth(
  BuildContext context, {
  required GitDiffResult result,
  required Map<GitDiffFile, _FullFileContents> fullFileContents,
  required Set<String> fullFilePreviewLimitedPaths,
  required GitDiffContentMode contentMode,
  required GitDiffPresentationMode presentationMode,
}) {
  var maxColumns = 1;

  void consider(String text) {
    maxColumns = math.max(maxColumns, _maxDiffDisplayColumns(text));
  }

  for (final file in result.files) {
    if (file.isBinary || file.isLarge || file.isGitlink) continue;
    final fileContentMode =
        contentMode == GitDiffContentMode.fullFile &&
            fullFilePreviewLimitedPaths.contains(file.path)
        ? GitDiffContentMode.diffOnly
        : contentMode;
    final contents = fullFileContents[file];
    var measuredFullFile = false;
    if (fileContentMode == GitDiffContentMode.fullFile) {
      if (presentationMode == GitDiffPresentationMode.sideBySide) {
        final oldContent = contents?.oldDecoded?.content;
        final newContent = contents?.newDecoded?.content;
        if (oldContent != null) {
          consider(oldContent);
          measuredFullFile = true;
        }
        if (newContent != null) {
          consider(newContent);
          measuredFullFile = true;
        }
      } else {
        final decoded = contents?.singleSideDecoded(file);
        if (decoded != null) {
          consider(decoded.content);
          measuredFullFile = true;
        }
      }
    }
    if (!measuredFullFile) {
      for (final line in _decodedDiffLines(file, contents)) {
        consider(line.text);
      }
    }
  }

  final style = AleraTokens.monoStyle.copyWith(fontSize: 12);
  final painter = TextPainter(
    text: TextSpan(text: 'M', style: style),
    textDirection: Directionality.of(context),
    maxLines: 1,
  )..layout();
  final textWidth = maxColumns * painter.width * 1.08;
  return presentationMode == GitDiffPresentationMode.sideBySide
      ? textWidth + 88.0
      : textWidth + 96.0;
}

class _DiffHorizontalViewport extends StatefulWidget {
  const _DiffHorizontalViewport({
    required this.scrollbarKey,
    required this.scrollStorageKey,
    required this.contentWidth,
    required this.child,
  });

  final String scrollbarKey;

  /// Identifies this scroll view's offset in the route's [PageStorage], so a
  /// diff tab rebuilt after a tab switch returns to where it was.
  final String scrollStorageKey;
  final double contentWidth;
  final Widget child;

  @override
  State<_DiffHorizontalViewport> createState() =>
      _DiffHorizontalViewportState();
}

class _DiffHorizontalViewportState extends State<_DiffHorizontalViewport> {
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
    return Scrollbar(
      key: ValueKey<String>(widget.scrollbarKey),
      controller: _scrollController,
      thumbVisibility: true,
      scrollbarOrientation: ScrollbarOrientation.bottom,
      notificationPredicate: (notification) =>
          notification.metrics.axis == Axis.horizontal,
      child: KeyedSubtree(
        key: PageStorageKey<String>(widget.scrollStorageKey),
        child: SingleChildScrollView(
          controller: _scrollController,
          scrollDirection: Axis.horizontal,
          child: SizedBox(width: widget.contentWidth, child: widget.child),
        ),
      ),
    );
  }
}

class _ReadOnlySideBySideDiffList extends StatefulWidget {
  const _ReadOnlySideBySideDiffList({
    super.key,
    required this.rows,
    required this.contentWidth,
    required this.scrollStorageKey,
  });

  final _DiffRows rows;
  final double contentWidth;
  final String scrollStorageKey;

  @override
  State<_ReadOnlySideBySideDiffList> createState() =>
      _ReadOnlySideBySideDiffListState();
}

class _ReadOnlySideBySideDiffListState
    extends State<_ReadOnlySideBySideDiffList> {
  late final ScrollController _leftHorizontalController;
  late final ScrollController _rightHorizontalController;
  late final ScrollController _leftVerticalController;
  late final ScrollController _rightVerticalController;
  var _syncingHorizontal = false;
  var _syncingVertical = false;

  @override
  void initState() {
    super.initState();
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
  void dispose() {
    _leftHorizontalController.dispose();
    _rightHorizontalController.dispose();
    _leftVerticalController.dispose();
    _rightVerticalController.dispose();
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
    final position = target.position;
    if (!position.hasContentDimensions) return;
    final nextOffset = source.offset.clamp(
      position.minScrollExtent,
      position.maxScrollExtent,
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

  Widget _buildPane({
    required bool isLeft,
    required ScrollController horizontalController,
    required ScrollController verticalController,
  }) {
    final side = isLeft ? 'left' : 'right';
    return LayoutBuilder(
      builder: (context, constraints) {
        final contentWidth = math.max(
          constraints.maxWidth,
          widget.contentWidth,
        );
        final list = ListView.builder(
          key: ValueKey<String>('git-diff-side-by-side-$side-list'),
          controller: verticalController,
          padding: const EdgeInsets.only(bottom: AleraTokens.space16),
          itemCount: widget.rows.length,
          itemBuilder: (context, index) => widget.rows
              .rowAt(index)
              .buildSideBySidePane(context, isLeft: isLeft),
        );
        return Scrollbar(
          key: ValueKey<String>('git-diff-side-by-side-$side-y-scrollbar'),
          controller: verticalController,
          thumbVisibility: true,
          notificationPredicate: (notification) =>
              notification.metrics.axis == Axis.vertical,
          child: Scrollbar(
            key: ValueKey<String>('git-diff-side-by-side-$side-x-scrollbar'),
            controller: horizontalController,
            thumbVisibility: true,
            scrollbarOrientation: ScrollbarOrientation.bottom,
            notificationPredicate: (notification) =>
                notification.metrics.axis == Axis.horizontal,
            child: KeyedSubtree(
              key: PageStorageKey<String>('${widget.scrollStorageKey}:$side:h'),
              child: SingleChildScrollView(
                controller: horizontalController,
                scrollDirection: Axis.horizontal,
                physics: const _NoImplicitHorizontalScrollPhysics(),
                child: SizedBox(
                  width: contentWidth,
                  height: constraints.maxHeight,
                  child: SelectionArea(
                    child: KeyedSubtree(
                      key: PageStorageKey<String>(
                        '${widget.scrollStorageKey}:$side:v',
                      ),
                      child: list,
                    ),
                  ),
                ),
              ),
            ),
          ),
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Expanded(
          child: _buildPane(
            isLeft: true,
            horizontalController: _leftHorizontalController,
            verticalController: _leftVerticalController,
          ),
        ),
        const SizedBox(
          width: 1,
          child: ColoredBox(color: AleraTokens.borderSubtle),
        ),
        Expanded(
          child: _buildPane(
            isLeft: false,
            horizontalController: _rightHorizontalController,
            verticalController: _rightVerticalController,
          ),
        ),
      ],
    );
  }
}

class _SingleColumnDiffList extends StatefulWidget {
  const _SingleColumnDiffList({
    super.key,
    required this.rows,
    required this.overview,
    required this.selectionEnabled,
    required this.contentWidth,
    required this.scrollStorageKey,
  });

  final _DiffRows rows;
  final _SingleColumnDiffOverviewProjection overview;
  final bool selectionEnabled;
  final double contentWidth;
  final String scrollStorageKey;

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
    return Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Expanded(
          child: LayoutBuilder(
            builder: (context, constraints) {
              final list = KeyedSubtree(
                key: PageStorageKey<String>('${widget.scrollStorageKey}:v'),
                child: ListView.builder(
                  key: const ValueKey<String>('git-diff-single-column-list'),
                  controller: _scrollController,
                  padding: const EdgeInsets.only(bottom: AleraTokens.space16),
                  itemCount: widget.rows.length,
                  itemBuilder: (context, index) =>
                      widget.rows.rowAt(index).build(context),
                ),
              );
              final selectable = widget.selectionEnabled
                  ? SelectionArea(child: list)
                  : list;
              return _DiffHorizontalViewport(
                scrollbarKey: 'git-diff-single-column-x-scrollbar',
                scrollStorageKey: '${widget.scrollStorageKey}:h',
                contentWidth: math.max(
                  constraints.maxWidth,
                  widget.contentWidth,
                ),
                child: SizedBox(
                  height: constraints.maxHeight,
                  child: selectable,
                ),
              );
            },
          ),
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
  required final _DiffSyntaxStyle Function(String filePath) syntaxForPath,
  // Scopes remembered scroll offsets to one diff tab. The modes are part of
  // the key so switching unified/side-by-side or diff-only/full-file does not
  // restore an offset measured against a different layout.
  required final String scrollStorageId,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final scrollStorageKey =
        '$scrollStorageId:${presentationMode.name}:${contentMode.name}';
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
        syntaxForPath: syntaxForPath,
        scrollStorageKey: scrollStorageKey,
      );
    }

    if (presentationMode == GitDiffPresentationMode.sideBySide) {
      return LayoutBuilder(
        builder: (context, constraints) {
          final rows = buildRows(
            editableViewportHeight: constraints.hasBoundedHeight
                ? constraints.maxHeight
                : null,
          );
          if (hasEditableSideBySide) {
            final contentWidth = math.max(constraints.maxWidth, 720.0);
            final list = KeyedSubtree(
              key: PageStorageKey<String>('$scrollStorageKey:editable:v'),
              child: ListView.builder(
                padding: const EdgeInsets.only(bottom: AleraTokens.space16),
                itemCount: rows.length,
                itemBuilder: (context, index) =>
                    rows.rowAt(index).build(context),
              ),
            );
            return KeyedSubtree(
              key: PageStorageKey<String>('$scrollStorageKey:editable:h'),
              child: SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: SizedBox(
                  width: contentWidth,
                  height: constraints.maxHeight,
                  child: list,
                ),
              ),
            );
          }
          // Keyed per mode: a mode switch gets a fresh list and controllers.
          return _ReadOnlySideBySideDiffList(
            key: ValueKey<String>(scrollStorageKey),
            rows: rows,
            scrollStorageKey: scrollStorageKey,
            contentWidth: _readOnlyDiffContentWidth(
              context,
              result: result,
              fullFileContents: fullFileContents,
              fullFilePreviewLimitedPaths: fullFilePreviewLimitedPaths,
              contentMode: contentMode,
              presentationMode: presentationMode,
            ),
          );
        },
      );
    }
    final rows = buildRows();
    // Keyed per mode: a mode switch gets a fresh list and controllers.
    return _SingleColumnDiffList(
      key: ValueKey<String>(scrollStorageKey),
      rows: rows,
      selectionEnabled: true,
      scrollStorageKey: scrollStorageKey,
      contentWidth: _readOnlyDiffContentWidth(
        context,
        result: result,
        fullFileContents: fullFileContents,
        fullFilePreviewLimitedPaths: fullFilePreviewLimitedPaths,
        contentMode: contentMode,
        presentationMode: presentationMode,
      ),
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
  _DiffLineRowSpan(this.lines)
    : intralineRanges = _buildIntralineRangesForDiffLines(lines);

  final List<GitDiffLine> lines;
  final List<List<_IntralineRange>> intralineRanges;

  @override
  int get length => lines.length;

  @override
  _DiffRow rowAt(int index) =>
      _DiffLineRow(lines[index], intralineRanges: intralineRanges[index]);
}

class _DiffRowsBuilder {
  final List<_DiffRowSpan> _spans = <_DiffRowSpan>[];
  final List<_DiffRow> _pending = <_DiffRow>[];
  _DiffSyntaxStyle? _syntax;

  set syntax(_DiffSyntaxStyle? value) {
    if (identical(_syntax, value)) return;
    _flushPending();
    _syntax = value;
  }

  _DiffRow _scopedRow(_DiffRow row) {
    final syntax = _syntax;
    return syntax == null
        ? row
        : _DiffSyntaxScopedRow(row: row, syntax: syntax);
  }

  _DiffRowSpan _scopedSpan(_DiffRowSpan span) {
    final syntax = _syntax;
    return syntax == null
        ? span
        : _DiffSyntaxScopedRowSpan(span: span, syntax: syntax);
  }

  void add(_DiffRow row) => _pending.add(_scopedRow(row));

  void addAll(Iterable<_DiffRow> rows) => _pending.addAll(rows.map(_scopedRow));

  void addSpans(Iterable<_DiffRowSpan> spans) {
    _flushPending();
    _spans.addAll(spans.where((span) => span.length > 0).map(_scopedSpan));
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
    required _DiffSyntaxStyle Function(String filePath) syntaxForPath,
    String? scrollStorageKey,
  }) {
    final items = _DiffRowsBuilder();
    if (result.truncated) {
      items.add(const _BannerRow('Diff truncated for preview.'));
    }
    for (final file in result.files) {
      final syntax = syntaxForPath(file.path);
      items.syntax = syntax;
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
                syntax: syntax,
                baseline: contents?.oldDecoded?.content ?? '',
                document: editable,
                whitespaceMode: whitespaceMode,
                viewportHeight: editableViewportHeight,
                onChanged: (text) => onEditableChanged(file, text),
                onSave: () => onEditableSave(file),
                scrollStorageKey: scrollStorageKey == null
                    ? null
                    : '$scrollStorageKey:file:${file.path}',
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

  Widget buildSideBySidePane(BuildContext context, {required bool isLeft}) =>
      build(context);
}

class const _WidgetDiffRow(final Widget child) extends _DiffRow {
  @override
  Widget build(BuildContext context) => child;
}

class const _FileHeaderRow(final GitDiffFile file, {final String? sourceLabel})
    extends _DiffRow {
  @override
  Widget build(BuildContext context) {
    final sourceLabelText = sourceLabel ?? file.sourceLabel ?? file.area.label;
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
                  '${context.tr(sourceLabelText)} · ${file.path}',
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

class const _DiffLineRow(
  final GitDiffLine text, {
  final List<_IntralineRange> intralineRanges = const <_IntralineRange>[],
}) extends _DiffRow {
  @override
  Widget build(BuildContext context) =>
      _DiffLine(line: text, intralineRanges: intralineRanges);
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

class const _DiffLine({
  required final GitDiffLine line,
  final List<_IntralineRange> intralineRanges = const <_IntralineRange>[],
}) extends StatelessWidget {
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
    final syntax = _DiffSyntaxScope.maybeOf(context);
    final syntaxHighlighted =
        syntax != null &&
        (line.kind == GitDiffLineKind.addition ||
            line.kind == GitDiffLineKind.deletion ||
            line.kind == GitDiffLineKind.context);
    final marker = syntaxHighlighted && line.text.isNotEmpty
        ? line.text.substring(0, 1)
        : '';
    final content = syntaxHighlighted ? _extractContent(line.text) : line.text;
    final baseStyle = AleraTokens.monoStyle.copyWith(
      fontSize: 12,
      color: syntax?.textStyle.color ?? color,
    );
    final isChangedLine =
        line.kind == GitDiffLineKind.addition ||
        line.kind == GitDiffLineKind.deletion;
    final changedContent = isChangedLine ? _extractContent(line.text) : content;
    final contentBaseSpan = syntaxHighlighted
        ? syntax.lineSpan(changedContent)
        : TextSpan(text: changedContent, style: baseStyle);
    final intralineBackground = line.kind == GitDiffLineKind.addition
        ? AleraTokens.success.withValues(alpha: 0.22)
        : AleraTokens.error.withValues(alpha: 0.22);
    final highlightedContent = isChangedLine
        ? _intralineHighlightedSpan(
            text: changedContent,
            baseSpan: contentBaseSpan,
            ranges: intralineRanges,
            background: intralineBackground,
          )
        : contentBaseSpan;
    final renderedSpan = isChangedLine
        ? TextSpan(
            style: baseStyle,
            children: <InlineSpan>[
              TextSpan(
                text: line.text.isEmpty ? '' : line.text.substring(0, 1),
                style: TextStyle(color: color),
              ),
              highlightedContent,
            ],
          )
        : syntaxHighlighted
        ? TextSpan(
            style: baseStyle,
            children: <InlineSpan>[
              TextSpan(
                text: marker,
                style: TextStyle(color: color),
              ),
              syntax.lineSpan(content),
            ],
          )
        : TextSpan(text: line.text, style: baseStyle);
    return DecoratedBox(
      decoration: BoxDecoration(color: background),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: AleraTokens.space12,
          vertical: AleraTokens.space2,
        ),
        child: Text.rich(
          renderedSpan,
          maxLines: 1,
          overflow: .visible,
          softWrap: false,
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
