part of 'workspace_git_diff_surface.dart';

class const _DiffFileList({
  required final GitDiffResult result,
  required final Map<GitDiffFile, _FullFileContents> fullFileContents,
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
    _DiffRows buildRows({double? editableViewportHeight}) {
      return _DiffRows.fromResult(
        result,
        fullFileContents: fullFileContents,
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
          return SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: SizedBox(
              width: contentWidth,
              height: constraints.maxHeight,
              child: ListView.builder(
                padding: const EdgeInsets.only(bottom: AleraTokens.space16),
                itemCount: rows.length,
                itemBuilder: (context, index) =>
                    rows.rowAt(index).build(context),
              ),
            ),
          );
        },
      );
    }
    final rows = buildRows();
    return ListView.builder(
      padding: const EdgeInsets.only(bottom: AleraTokens.space16),
      itemCount: rows.length,
      itemBuilder: (context, index) => rows.rowAt(index).build(context),
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
        final editable = editableDocuments[file.path];
        if (sideBySide &&
            contentMode == GitDiffContentMode.fullFile &&
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
        if (contentMode == GitDiffContentMode.fullFile) {
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
          items.addAll(
            _buildSideBySideRows(
              _fileWithDecodedDiffLines(file, fullFileContents[file]),
            ),
          );
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
    return DecoratedBox(
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
