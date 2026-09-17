import 'dart:math' show max;

import 'package:alera/src/features/workbench/domain/terminal_search.dart';
import 'package:flutter/foundation.dart';
import 'package:xterm2/xterm.dart' as xterm;

import 'terminal_search_source.dart';

typedef TerminalSearchLineScroller = void Function(int lineIndex);

final class TerminalSearchController._(
  var TerminalSearchSource _source,
  final TerminalSearchLineScroller _scrollToLine,
) extends ChangeNotifier {
  factory({
    required xterm.Terminal terminal,
    required TerminalSearchLineScroller scrollToLine,
  }) {
    return TerminalSearchController._(
      XtermTerminalSearchSource(terminal),
      scrollToLine,
    );
  }

  factory TerminalSearchController.fromSource({
    required TerminalSearchSource source,
    required TerminalSearchLineScroller scrollToLine,
  }) {
    return TerminalSearchController._(source, scrollToLine);
  }

  Object? _indexedBuffer;
  int _indexedHeight = -1;
  int _indexedWidth = -1;
  bool _needsFullRefresh = true;
  bool _isOpen = false;
  bool _listeningToTerminal = false;
  String _query = '';
  int _selectedIndex = -1;
  final Map<Object, List<_LineMatch>> _matchesByLine =
      <Object, List<_LineMatch>>{};
  List<TerminalSearchMatch> _matches = const <TerminalSearchMatch>[];

  bool get isOpen => _isOpen;

  String get query => _query;

  List<TerminalSearchMatch> get matches => _matches;

  int get matchCount => _matches.length;

  int? get selectedMatchNumber {
    if (_selectedIndex < 0 || _selectedIndex >= _matches.length) {
      return null;
    }
    return _selectedIndex + 1;
  }

  TerminalSearchMatch? get selectedMatch {
    if (_selectedIndex < 0 || _selectedIndex >= _matches.length) {
      return null;
    }
    return _matches[_selectedIndex];
  }

  void open() {
    if (_isOpen) {
      return;
    }
    _isOpen = true;
    _syncTerminalListener();
    if (_query.isNotEmpty) {
      _refresh(forceFull: _needsFullRefresh);
    }
    notifyListeners();
  }

  void close() {
    if (!_isOpen) {
      return;
    }
    _isOpen = false;
    _syncTerminalListener();
    // Keep the query for the next invocation, but make reopening authoritative
    // after output that arrived while the overlay was hidden. The match index
    // is released because reopening rescans anyway; keeping it would retain
    // one entry per scrollback hit while nobody can see them.
    _needsFullRefresh = true;
    _matchesByLine.clear();
    _matches = const <TerminalSearchMatch>[];
    _selectedIndex = -1;
    notifyListeners();
  }

  void setQuery(String query) {
    if (_query == query) {
      return;
    }
    _query = query;
    _syncTerminalListener();
    _selectedIndex = -1;
    _matchesByLine.clear();
    _matches = const <TerminalSearchMatch>[];
    _needsFullRefresh = true;
    if (_query.isNotEmpty) {
      _refresh(forceFull: true);
      if (_matches.isNotEmpty) {
        _selectedIndex = 0;
        _scrollSelectedMatch();
      }
    }
    notifyListeners();
  }

  void next() {
    if (_matches.isEmpty) {
      return;
    }
    _selectedIndex = (_selectedIndex + 1) % _matches.length;
    _scrollSelectedMatch();
    notifyListeners();
  }

  void previous() {
    if (_matches.isEmpty) {
      return;
    }
    _selectedIndex = (_selectedIndex - 1) % _matches.length;
    if (_selectedIndex < 0) {
      _selectedIndex = _matches.length - 1;
    }
    _scrollSelectedMatch();
    notifyListeners();
  }

  /// Reattaches the search index when a snapshot replaces the emulator.
  void attachTerminal(xterm.Terminal terminal) {
    attachSource(XtermTerminalSearchSource(terminal));
  }

  void attachSource(TerminalSearchSource source) {
    if (identical(_source, source)) return;
    if (_listeningToTerminal) {
      _source.removeListener(_handleTerminalChanged);
    }
    _source = source;
    if (_listeningToTerminal) {
      _source.addListener(_handleTerminalChanged);
    }
    _indexedBuffer = null;
    _indexedHeight = -1;
    _indexedWidth = -1;
    _needsFullRefresh = true;
    if (_isOpen && _query.isNotEmpty) {
      _refresh(forceFull: true);
      notifyListeners();
    }
  }

  @visibleForTesting
  bool get needsFullRefreshForTesting => _needsFullRefresh;

  void _syncTerminalListener() {
    final shouldListen = _isOpen && _query.isNotEmpty;
    if (shouldListen == _listeningToTerminal) {
      return;
    }
    if (shouldListen) {
      _source.addListener(_handleTerminalChanged);
    } else {
      _source.removeListener(_handleTerminalChanged);
    }
    _listeningToTerminal = shouldListen;
  }

  void _handleTerminalChanged() {
    if (!_isOpen || _query.isEmpty) {
      _needsFullRefresh = true;
      return;
    }
    if (_refresh()) {
      notifyListeners();
    }
  }

  bool _refresh({bool forceFull = false}) {
    if (_query.isEmpty) {
      final hadMatches = _matches.isNotEmpty || _matchesByLine.isNotEmpty;
      _matchesByLine.clear();
      _matches = const <TerminalSearchMatch>[];
      _selectedIndex = -1;
      return hadMatches;
    }

    final height = _source.height;
    final shouldScanAll =
        forceFull ||
        _needsFullRefresh ||
        !identical(_indexedBuffer, _source.bufferIdentity) ||
        _indexedWidth != _source.viewWidth ||
        _indexedHeight < 0 ||
        height < _indexedHeight;
    final selected = selectedMatch;

    if (shouldScanAll) {
      _matchesByLine.clear();
      _scanLines(0, height);
    } else {
      // A normal output batch appends from the previous tail. When the line
      // count is stable, rescan only the visible tail because TUIs rewrite
      // their viewport instead of the whole scrollback.
      final start = height > _indexedHeight
          ? max(0, _indexedHeight - 1)
          : max(0, height - _source.viewHeight);
      _removeMatchesInRange(start, height);
      _scanLines(start, height);
    }

    _indexedBuffer = _source.bufferIdentity;
    _indexedHeight = height;
    _indexedWidth = _source.viewWidth;
    _needsFullRefresh = false;
    _rebuildMatches(selected);
    return true;
  }

  void _scanLines(int start, int end) {
    final safeStart = start.clamp(0, _source.height);
    final safeEnd = end.clamp(safeStart, _source.height);
    for (var index = safeStart; index < safeEnd; index++) {
      final line = _source.lineIdAt(index);
      final lineMatches = findTerminalSearchMatches(<TerminalSearchLine>[
        TerminalSearchLine(
          id: line,
          index: index,
          text: _source.lineTextAt(index),
        ),
      ], _query);
      if (lineMatches.isEmpty) {
        _matchesByLine.remove(line);
      } else {
        _matchesByLine[line] = <_LineMatch>[
          for (final match in lineMatches)
            _LineMatch(start: match.start, end: match.end),
        ];
      }
    }
  }

  void _removeMatchesInRange(int start, int end) {
    final safeStart = start.clamp(0, _source.height);
    final safeEnd = end.clamp(safeStart, _source.height);
    for (var index = safeStart; index < safeEnd; index++) {
      _matchesByLine.remove(_source.lineIdAt(index));
    }
  }

  void _rebuildMatches(TerminalSearchMatch? selected) {
    final next = <TerminalSearchMatch>[];
    final staleLines = <Object>[];
    final liveLines =
        <({Object line, int lineIndex, List<_LineMatch> matches})>[];
    for (final entry in _matchesByLine.entries) {
      final lineIndex = _source.lineIndexOf(entry.key);
      if (lineIndex == null) {
        staleLines.add(entry.key);
        continue;
      }
      liveLines.add((
        line: entry.key,
        lineIndex: lineIndex,
        matches: entry.value,
      ));
    }
    for (final line in staleLines) {
      _matchesByLine.remove(line);
    }

    // `findTerminalSearchMatches` records hits in ascending column order for
    // each line. Sorting the matched lines first therefore produces the same
    // global order without sorting every individual hit. This matters for a
    // query that occurs many times per line during continuous terminal output.
    liveLines.sort((a, b) => a.lineIndex.compareTo(b.lineIndex));
    for (final entry in liveLines) {
      for (final match in entry.matches) {
        next.add(
          TerminalSearchMatch(
            lineId: entry.line,
            lineIndex: entry.lineIndex,
            start: match.start,
            end: match.end,
          ),
        );
      }
    }
    _matches = List<TerminalSearchMatch>.unmodifiableOf(next);

    if (_matches.isEmpty) {
      _selectedIndex = -1;
      return;
    }
    if (selected != null) {
      final retainedIndex = _matches.indexWhere(
        (match) =>
            identical(match.lineId, selected.lineId) &&
            match.start == selected.start,
      );
      if (retainedIndex >= 0) {
        _selectedIndex = retainedIndex;
        return;
      }
    }
    _selectedIndex = _selectedIndex.clamp(0, _matches.length - 1);
  }

  void _scrollSelectedMatch() {
    final match = selectedMatch;
    if (match != null) {
      _scrollToLine(match.lineIndex);
    }
  }

  @override
  void dispose() {
    if (_listeningToTerminal) {
      _source.removeListener(_handleTerminalChanged);
      _listeningToTerminal = false;
    }
    super.dispose();
  }
}

final class const _LineMatch({
  required final int start,
  required final int end,
});

extension on int? {
  bool caseInRange(int start, int end) {
    final value = this;
    return value != null && value >= start && value < end;
  }
}
