/// A line of terminal text that can be searched without retaining the whole
/// terminal buffer as a second string.
final class const TerminalSearchLine({
  required final Object id,
  required final int index,
  required final String text,
});

/// A literal occurrence in one terminal buffer line.
final class const TerminalSearchMatch({
  required final Object lineId,
  required final int lineIndex,
  required final int start,
  required final int end,
});

/// A prepared, reusable literal query for searching a large terminal buffer.
///
/// Match offsets remain UTF-16 indices and case folding is identical to the
/// original implementation. Most rows contain no match, so avoid allocating
/// one result list and wrapper objects for each such scrollback line.
final class TerminalLiteralQuery {
  TerminalLiteralQuery(String query, {bool caseSensitive = false})
    : _needle = caseSensitive ? query : query.toLowerCase(),
      _caseSensitive = caseSensitive;

  final String _needle;
  final bool _caseSensitive;

  int get matchLength => _needle.length;

  List<int> matchStarts(String text) {
    if (_needle.isEmpty) return const <int>[];
    final haystack = _caseSensitive ? text : text.toLowerCase();
    var matchStart = haystack.indexOf(_needle);
    if (matchStart < 0) return const <int>[];
    final starts = <int>[];
    do {
      starts.add(matchStart);
      matchStart = haystack.indexOf(_needle, matchStart + _needle.length);
    } while (matchStart >= 0);
    return starts;
  }
}

/// Finds non-overlapping literal matches in [lines].
///
/// Terminal search is case-insensitive by default. Regex syntax is never
/// interpreted, so a query such as `[` is searched literally.
List<TerminalSearchMatch> findTerminalSearchMatches(
  Iterable<TerminalSearchLine> lines,
  String query, {
  bool caseSensitive = false,
}) {
  if (query.isEmpty) {
    return const <TerminalSearchMatch>[];
  }

  final matcher = TerminalLiteralQuery(query, caseSensitive: caseSensitive);
  final matches = <TerminalSearchMatch>[];
  for (final line in lines) {
    for (final matchStart in matcher.matchStarts(line.text)) {
      matches.add(
        TerminalSearchMatch(
          lineId: line.id,
          lineIndex: line.index,
          start: matchStart,
          end: matchStart + matcher.matchLength,
        ),
      );
    }
  }
  return matches;
}
