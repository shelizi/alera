import 'dart:async';

import 'package:flutter/foundation.dart' show compute;

import 'package:flutter/material.dart';

import 'controller.dart';
import 'styling.dart';

typedef RegexSearchRequest = ({
  String text,
  String query,
  bool caseSensitive,
  bool matchWholeWord,
});

List<(int start, int end)> computeRegexSearchRanges(
  RegexSearchRequest request,
) {
  var pattern = request.query;
  if (request.matchWholeWord) {
    pattern = r'\b' + pattern + r'\b';
  }
  final regExp = RegExp(
    pattern,
    caseSensitive: request.caseSensitive,
    multiLine: true,
  );

  final ranges = <(int start, int end)>[];
  var utf16Cursor = 0;
  var scalarCursor = 0;

  int scalarOffsetAt(int targetUtf16) {
    while (utf16Cursor < targetUtf16) {
      final first = request.text.codeUnitAt(utf16Cursor);
      if (first >= 0xD800 &&
          first <= 0xDBFF &&
          utf16Cursor + 1 < request.text.length) {
        final second = request.text.codeUnitAt(utf16Cursor + 1);
        utf16Cursor += second >= 0xDC00 && second <= 0xDFFF ? 2 : 1;
      } else {
        utf16Cursor++;
      }
      scalarCursor++;
    }
    return scalarCursor;
  }

  for (final match in regExp.allMatches(request.text)) {
    final start = scalarOffsetAt(match.start);
    final end = scalarOffsetAt(match.end);
    ranges.add((start, end));
  }
  return ranges;
}

typedef ReplaceAllRequest = ({
  String text,
  String query,
  String replacement,
  bool isRegex,
  bool caseSensitive,
  bool matchWholeWord,
});

String computeReplaceAllText(ReplaceAllRequest request) {
  var pattern = request.isRegex ? request.query : RegExp.escape(request.query);
  if (request.matchWholeWord) {
    pattern = '\\b$pattern\\b';
  }
  final regExp = RegExp(pattern, caseSensitive: request.caseSensitive);
  return request.text.replaceAll(regExp, request.replacement);
}

class _FindMatch {
  const _FindMatch({required this.start, required this.end});

  final int start;
  final int end;
}

/// Controller for managing text search functionality in [CodeForge].
///
/// This controller handles searching for text, navigating through matches,
/// and highlighting results in the editor.
class FindController extends ChangeNotifier {
  final CodeForgeController _codeController;

  List<_FindMatch> _matches = [];
  int _currentMatchIndex = -1;
  bool _isRegex = false;
  bool _caseSensitive = false;
  bool _matchWholeWord = false;
  String _lastQuery = '';
  bool _isActive = false;
  bool _isReplaceMode = false;
  static const _searchDebounceDuration = Duration(milliseconds: 80);
  Timer? _searchDebounce;
  int _searchRequestSerial = 0;
  bool _disposed = false;

  int _lastDocumentVersion = -1;
  VoidCallback? _controllerListener;

  final TextEditingController findInputController = TextEditingController();
  final TextEditingController replaceInputController = TextEditingController();
  final FocusNode findInputFocusNode = FocusNode();
  final FocusNode replaceInputFocusNode = FocusNode();

  /// Creates a [FindController] associated with the given [CodeForgeController].
  FindController(this._codeController) {
    _lastDocumentVersion = _codeController.documentVersion;
    _controllerListener = _onCodeControllerChanged;
    _codeController.addListener(_controllerListener!);
    findInputController.addListener(_onFindInputChanged);
  }

  void _onFindInputChanged() {
    find(findInputController.text);
  }

  @override
  void dispose() {
    _disposed = true;
    _searchDebounce?.cancel();
    _searchRequestSerial++;
    if (_controllerListener != null) {
      _codeController.removeListener(_controllerListener!);
    }
    findInputController.removeListener(_onFindInputChanged);
    findInputController.dispose();
    replaceInputController.dispose();
    super.dispose();
  }

  void _onCodeControllerChanged() {
    if (!_isActive && _lastQuery.isEmpty) return;
    final currentVersion = _codeController.documentVersion;
    if (currentVersion != _lastDocumentVersion) {
      _lastDocumentVersion = currentVersion;
      _reperformSearch();
    }
  }

  /// The number of matches found for the current query.
  int get matchCount => _matches.length;

  /// The current match index (0-based) or -1 if no match is selected.
  int get currentMatchIndex => _currentMatchIndex;

  /// The case sensitivity of the search.
  bool get caseSensitive => _caseSensitive;

  /// Whether the search uses regular expressions.
  bool get isRegex => _isRegex;

  /// Whether the search matches whole words only.
  bool get matchWholeWord => _matchWholeWord;

  /// Whether the finder is currently active/visible.
  bool get isActive => _isActive;

  /// Whether the replace mode is active.
  bool get isReplaceMode => _isReplaceMode;

  /// Sets the case sensitivity of the search.
  set caseSensitive(bool value) {
    if (_caseSensitive == value) return;
    _caseSensitive = value;
    _reperformSearch();
    notifyListeners();
  }

  /// Sets whether the search uses regular expressions.
  set isRegex(bool value) {
    if (_isRegex == value) return;
    _isRegex = value;
    _reperformSearch();
    notifyListeners();
  }

  /// Sets whether the search matches whole words only.
  set matchWholeWord(bool value) {
    if (_matchWholeWord == value) return;
    _matchWholeWord = value;
    _reperformSearch();
    notifyListeners();
  }

  /// Sets whether the finder is currently active/visible.
  set isActive(bool value) {
    if (_isActive == value) return;
    _isActive = value;
    if (_isActive) {
      Future.microtask(() => findInputFocusNode.requestFocus());
      if (_lastQuery.isNotEmpty) {
        _reperformSearch();
      }
    } else {
      _clearMatches();
    }
    notifyListeners();
  }

  /// Sets whether the replace mode is active.
  set isReplaceMode(bool value) {
    if (_isReplaceMode == value) return;
    _isReplaceMode = value;
    notifyListeners();
  }

  void toggleReplaceMode() {
    isReplaceMode = !isReplaceMode;
  }

  void toggleActive() {
    isActive = !isActive;
  }

  void toggleCaseSensitive() {
    caseSensitive = !caseSensitive;
  }

  void toggleRegex() {
    isRegex = !isRegex;
  }

  void toggleMatchWholeWord() {
    matchWholeWord = !matchWholeWord;
  }

  void _reperformSearch() {
    if (_lastQuery.isNotEmpty) {
      find(_lastQuery, scrollToMatch: false);
    }
  }

  /// Performs a text search.
  ///
  /// [query] is the text to search for.
  /// [scrollToMatch] determines if the editor should scroll to the selected match.
  void find(String query, {bool scrollToMatch = true}) {
    _lastQuery = query;
    _searchDebounce?.cancel();
    final requestSerial = ++_searchRequestSerial;

    if (query.isEmpty) {
      _clearMatches(invalidatePendingSearch: false);
      return;
    }

    final documentVersion = _codeController.documentVersion;
    _lastDocumentVersion = documentVersion;
    final caseSensitive = _caseSensitive;
    final matchWholeWord = _matchWholeWord;
    final isRegex = _isRegex;

    _clearMatches(invalidatePendingSearch: false);
    _searchDebounce = Timer(_searchDebounceDuration, () {
      unawaited(
        isRegex
            ? _runRegexSearch(
                requestSerial: requestSerial,
                documentVersion: documentVersion,
                query: query,
                caseSensitive: caseSensitive,
                matchWholeWord: matchWholeWord,
                scrollToMatch: scrollToMatch,
              )
            : _runLiteralSearch(
                requestSerial: requestSerial,
                documentVersion: documentVersion,
                query: query,
                caseSensitive: caseSensitive,
                matchWholeWord: matchWholeWord,
                scrollToMatch: scrollToMatch,
              ),
      );
    });
  }

  Future<void> _runLiteralSearch({
    required int requestSerial,
    required int documentVersion,
    required String query,
    required bool caseSensitive,
    required bool matchWholeWord,
    required bool scrollToMatch,
  }) async {
    try {
      _codeController.flushPendingBuffer();
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      final ranges = await _codeController.rope.findLiteral(
        query,
        caseSensitive: caseSensitive,
        matchWholeWord: matchWholeWord,
      );
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      final matches = ranges
          .map((match) => _FindMatch(start: match.$1, end: match.$2))
          .toList(growable: false);
      _applyMatches(matches, scrollToMatch: scrollToMatch);
    } catch (e) {
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      _applyMatches(const [], scrollToMatch: false);
    }
  }

  Future<void> _runRegexSearch({
    required int requestSerial,
    required int documentVersion,
    required String query,
    required bool caseSensitive,
    required bool matchWholeWord,
    required bool scrollToMatch,
  }) async {
    try {
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      _codeController.flushPendingBuffer();
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      final text = await _codeController.rope.getTextSnapshot();
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      final ranges = await compute(computeRegexSearchRanges, (
        text: text,
        query: query,
        caseSensitive: caseSensitive,
        matchWholeWord: matchWholeWord,
      ));
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      final matches = ranges
          .map((match) => _FindMatch(start: match.$1, end: match.$2))
          .toList(growable: false);
      _applyMatches(matches, scrollToMatch: scrollToMatch);
    } catch (e) {
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      _applyMatches(const [], scrollToMatch: false);
    }
  }

  bool _isSearchRequestCurrent(int requestSerial, int documentVersion) {
    return !_disposed &&
        requestSerial == _searchRequestSerial &&
        documentVersion == _codeController.documentVersion;
  }

  void _applyMatches(List<_FindMatch> matches, {required bool scrollToMatch}) {
    _matches = matches;
    if (_matches.isEmpty) {
      _currentMatchIndex = -1;
      _updateHighlights();
      return;
    }

    final cursor = _codeController.selection.start;
    var index = 0;
    var found = false;

    for (var i = 0; i < _matches.length; i++) {
      if (_matches[i].start >= cursor) {
        index = i;
        found = true;
        break;
      }
    }

    _currentMatchIndex = found ? index : 0;
    _updateHighlights();
    if (scrollToMatch) {
      _scrollToCurrentMatch();
    }
  }

  /// Moves to the next match.
  void next() {
    if (_matches.isEmpty) return;
    _currentMatchIndex = (_currentMatchIndex + 1) % _matches.length;
    _scrollToCurrentMatch();
    _updateHighlights();
  }

  /// Moves to the previous match.
  void previous() {
    if (_matches.isEmpty) return;
    _currentMatchIndex =
        (_currentMatchIndex - 1 + _matches.length) % _matches.length;
    _scrollToCurrentMatch();
    _updateHighlights();
  }

  /// Clears search results and highlights.
  void clear() {
    _lastQuery = '';
    _clearMatches();
  }

  /// Replaces the currently selected match with the text in [replaceInputController].
  void replace() {
    if (_currentMatchIndex < 0 || _currentMatchIndex >= _matches.length) return;

    final match = _matches[_currentMatchIndex];
    _codeController.replaceRange(
      match.start,
      match.end,
      replaceInputController.text,
    );
  }

  /// Replaces all matches with the text in [replaceInputController].
  void replaceAll() {
    if (_matches.isEmpty || _lastQuery.isEmpty) return;

    _searchDebounce?.cancel();
    final requestSerial = ++_searchRequestSerial;
    final documentVersion = _codeController.documentVersion;
    final query = _lastQuery;
    final replacement = replaceInputController.text;
    final isRegex = _isRegex;
    final caseSensitive = _caseSensitive;
    final matchWholeWord = _matchWholeWord;

    unawaited(
      _runReplaceAll(
        requestSerial: requestSerial,
        documentVersion: documentVersion,
        query: query,
        replacement: replacement,
        isRegex: isRegex,
        caseSensitive: caseSensitive,
        matchWholeWord: matchWholeWord,
      ),
    );
  }

  Future<void> _runReplaceAll({
    required int requestSerial,
    required int documentVersion,
    required String query,
    required String replacement,
    required bool isRegex,
    required bool caseSensitive,
    required bool matchWholeWord,
  }) async {
    try {
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      _codeController.flushPendingBuffer();
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      final text = await _codeController.rope.getTextSnapshot();
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      final newText = await compute(computeReplaceAllText, (
        text: text,
        query: query,
        replacement: replacement,
        isRegex: isRegex,
        caseSensitive: caseSensitive,
        matchWholeWord: matchWholeWord,
      ));
      if (!_isSearchRequestCurrent(requestSerial, documentVersion)) return;
      if (replacement != replaceInputController.text) return;
      if (newText == text) return;
      _codeController.replaceRange(0, _codeController.length, newText);
    } catch (e) {
      if (_disposed) return;
      debugPrint('FindController: Replace All failed. Error: $e');
    }
  }

  void _clearMatches({bool invalidatePendingSearch = true}) {
    _searchDebounce?.cancel();
    if (invalidatePendingSearch) {
      _searchRequestSerial++;
    }
    _matches = [];
    _currentMatchIndex = -1;
    _codeController.searchHighlights = [];
    _codeController.searchHighlightsChanged = true;
    _codeController.notifyListeners();
    notifyListeners();
  }

  void _scrollToCurrentMatch() {
    if (_currentMatchIndex >= 0 && _currentMatchIndex < _matches.length) {
      final match = _matches[_currentMatchIndex];
      final matchLine = _codeController.getLineAtOffset(match.start);
      _codeController.setSelectionSilently(
        TextSelection.collapsed(offset: match.start),
      );

      try {
        _codeController.scrollToLine(matchLine);
      } on StateError {
        //
      }
    }
  }

  void _updateHighlights() {
    final highlights = <SearchHighlight>[];

    for (int i = 0; i < _matches.length; i++) {
      final match = _matches[i];
      final isCurrent = i == _currentMatchIndex;

      highlights.add(
        SearchHighlight(
          start: match.start,
          end: match.end,
          isCurrentMatch: isCurrent,
        ),
      );
    }

    _codeController.searchHighlights = highlights;
    _codeController.searchHighlightsChanged = true;
    _codeController.notifyListeners();
    notifyListeners();
  }
}
