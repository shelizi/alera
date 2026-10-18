part of 'workspace_git_diff_surface.dart';

typedef _DiffNavigationTarget = ({
  int row,
  String? editablePath,
  int? editableLine,
});

/// Shared by the toolbar and the active diff viewport. Targets are actual
/// rendered-row indexes, not approximate scroll percentages; unchanged full
/// file spans can represent hundreds of thousands of rows without traversal.
final class _DiffChangeNavigator extends ChangeNotifier {
  String? _identity;
  List<_DiffNavigationTarget> _targets = const [];
  int _selected = 0;

  int get count => _targets.length;
  int get selected => _selected;
  bool get canPrevious => _selected > 0;
  bool get canNext => _selected + 1 < _targets.length;
  _DiffNavigationTarget? get target =>
      _targets.isEmpty ? null : _targets[_selected];

  void update(String identity, List<_DiffNavigationTarget> targets) {
    final same =
        _identity == identity &&
        _targets.length == targets.length &&
        Iterable<int>.generate(targets.length)
            .every((index) => _targets[index] == targets[index]);
    if (same) return;
    final newDiff = _identity != identity;
    _identity = identity;
    _targets = List<_DiffNavigationTarget>.unmodifiable(targets);
    _selected = newDiff
        ? 0
        : _selected.clamp(0, targets.isEmpty ? 0 : targets.length - 1);
    notifyListeners();
  }

  void reset() {
    if (_targets.isEmpty && _identity == null) return;
    _identity = null;
    _targets = const [];
    _selected = 0;
    notifyListeners();
  }

  void navigate(int direction) {
    if (_targets.isEmpty) return;
    final next = (_selected + direction).clamp(0, _targets.length - 1);
    if (next == _selected) return;
    _selected = next;
    notifyListeners();
  }
}

/// Extract hunk/changed-block starts from the lazy diff row spans. We never
/// materialize unchanged full-file ranges just to find the next change.
List<_DiffNavigationTarget> _diffNavigationTargets(List<_DiffRowSpan> spans) {
  final result = <_DiffNavigationTarget>[];
  var rowOffset = 0;
  var inChange = false;

  void mark(
    int row, {
    String? editablePath,
    int? editableLine,
    bool hunk = false,
  }) {
    if (hunk || !inChange) {
      final target = (
        row: row,
        editablePath: editablePath,
        editableLine: editableLine,
      );
      if (result.isEmpty || result.last != target) result.add(target);
    }
    inChange = true;
  }

  void visitKind(GitDiffLineKind? kind, int row) {
    switch (kind) {
      case GitDiffLineKind.hunk:
        mark(row, hunk: true);
      case GitDiffLineKind.addition:
      case GitDiffLineKind.deletion:
        mark(row);
      case GitDiffLineKind.header:
      case GitDiffLineKind.context:
      case null:
        inChange = false;
    }
  }

  void visitRow(_DiffRow row, int index) {
    if (row is _DiffSyntaxScopedRow) {
      visitRow(row.row, index);
    } else if (row is _DiffLineRow) {
      visitKind(row.text.kind, index);
    } else if (row is _FullFileDiffRow) {
      visitKind(row.line.kind, index);
    } else if (row is _SideBySideDiffRow) {
      final left = row.left?.kind;
      final right = row.right?.kind;
      visitKind(
        left == GitDiffLineKind.deletion || right == GitDiffLineKind.addition
            ? GitDiffLineKind.addition
            : left ?? right,
        index,
      );
    } else if (row is _WidgetDiffRow && row.child is _EditableWorkingTreeDiff) {
      final editor = row.child as _EditableWorkingTreeDiff;
      final hunks = editor.file.lines
          .where((line) => line.kind == GitDiffLineKind.hunk)
          .toList(growable: false);
      if (hunks.isEmpty) {
        if (editor.file.lines.any(
          (line) =>
              line.kind == GitDiffLineKind.addition ||
              line.kind == GitDiffLineKind.deletion,
        )) {
          mark(index, editablePath: editor.file.path, editableLine: 0);
        }
      } else {
        for (final hunk in hunks) {
          final match = _hunkHeaderRegExp.firstMatch(hunk.text);
          final start =
              int.tryParse(match?.group(3) ?? '') ??
              int.tryParse(match?.group(1) ?? '') ??
              1;
          inChange = false;
          mark(
            index,
            editablePath: editor.file.path,
            editableLine: math.max(start - 1, 0),
            hunk: true,
          );
        }
      }
      inChange = false;
    } else {
      inChange = false;
    }
  }

  void scan(_DiffRowSpan span, int start) {
    if (span is _DiffSyntaxScopedRowSpan) {
      scan(span.span, start);
    } else if (span is _FullFileContextDiffRowSpan ||
        span is _FullFileSideBySideContextDiffRowSpan) {
      inChange = false;
    } else if (span is _FullFileForcedDiffRowSpan ||
        span is _FullFileSideBySideForcedDiffRowSpan) {
      if (span.length > 0) mark(start);
    } else if (span is _DiffLineRowSpan) {
      for (var i = 0; i < span.lines.length; i++) {
        visitKind(span.lines[i].kind, start + i);
      }
    } else if (span is _ProjectedSideBySideDiffRowSpan) {
      final file = span.file;
      for (var i = 0; i < file.sideBySideRows.length; i++) {
        final projected = file.sideBySideRows[i];
        if (projected.kind == GitDiffSideBySideRowKind.passthrough) {
          final index = projected.lineIndex;
          visitKind(index == null ? null : file.lines[index].kind, start + i);
        } else {
          final oldIndex = projected.leftLineIndex;
          final newIndex = projected.rightLineIndex;
          final left = oldIndex == null ? null : file.lines[oldIndex].kind;
          final right = newIndex == null ? null : file.lines[newIndex].kind;
          visitKind(
            left == GitDiffLineKind.deletion ||
                    right == GitDiffLineKind.addition
                ? GitDiffLineKind.addition
                : left ?? right,
            start + i,
          );
        }
      }
    } else if (span is _MaterializedDiffRowSpan) {
      for (var i = 0; i < span.rows.length; i++) {
        visitRow(span.rows[i], start + i);
      }
    } else {
      // Future span types should supply a specialized lazy case above.
      inChange = false;
    }
  }

  for (final span in spans) {
    scan(span, rowOffset);
    rowOffset += span.length;
  }
  return result;
}
