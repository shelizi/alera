import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:flutter/material.dart';

class const AleraSearchTextMatch({
  required final int start,
  required final int end,
});

/// Ordered-subsequence matching: "mnd" matches "main.dart", but "mdn"
/// does not. Used for filename/path filters alongside Rust Quick Open.
bool aleraFuzzyTextContains(
  String text,
  String query, {
  bool caseSensitive = false,
}) {
  final needle = caseSensitive ? query.trim() : query.trim().toLowerCase();
  if (needle.isEmpty) return true;
  final source = caseSensitive ? text : text.toLowerCase();
  var matched = 0;
  for (final unit in source.codeUnits) {
    if (unit != needle.codeUnitAt(matched)) continue;
    if (++matched == needle.length) return true;
  }
  return false;
}

/// Highlight ordered fuzzy characters when a contiguous match is absent.
List<AleraSearchTextMatch> aleraFuzzyTextMatches(
  String text,
  String query, {
  bool caseSensitive = false,
}) {
  final direct = aleraSearchTextMatches(
    text,
    query,
    caseSensitive: caseSensitive,
  );
  if (direct.isNotEmpty) return direct;
  final needle = caseSensitive ? query.trim() : query.trim().toLowerCase();
  if (needle.isEmpty) return const <AleraSearchTextMatch>[];
  final source = caseSensitive ? text : text.toLowerCase();
  var matched = 0;
  var start = -1;
  var end = -1;
  final spans = <AleraSearchTextMatch>[];
  for (
    var index = 0;
    index < source.length && matched < needle.length;
    index++
  ) {
    if (source.codeUnitAt(index) != needle.codeUnitAt(matched)) continue;
    if (start < 0) {
      start = index;
    } else if (index != end) {
      spans.add(AleraSearchTextMatch(start: start, end: end));
      start = index;
    }
    end = index + 1;
    matched++;
  }
  if (matched != needle.length) return const <AleraSearchTextMatch>[];
  spans.add(AleraSearchTextMatch(start: start, end: end));
  return spans;
}

List<AleraSearchTextMatch> aleraSearchTextMatches(
  String text,
  String query, {
  bool caseSensitive = false,
}) {
  final needle = query.trim();
  if (needle.isEmpty || text.isEmpty) {
    return const <AleraSearchTextMatch>[];
  }
  final pattern = RegExp(RegExp.escape(needle), caseSensitive: caseSensitive);
  return <AleraSearchTextMatch>[
    for (final match in pattern.allMatches(text))
      AleraSearchTextMatch(start: match.start, end: match.end),
  ];
}

TextStyle aleraSearchMatchStyle(TextStyle baseStyle, {bool active = false}) {
  return baseStyle.copyWith(
    color: active ? AleraTokens.onAccent : AleraTokens.foreground,
    backgroundColor: active ? AleraTokens.accent : AleraTokens.accentSubtle,
    fontWeight: FontWeight.w700,
  );
}

TextSpan aleraSearchHighlightedSpan({
  required String text,
  required String query,
  required TextStyle baseStyle,
  bool caseSensitive = false,
  bool fuzzy = false,
  int activeMatchIndex = -1,
}) {
  final matches = fuzzy
      ? aleraFuzzyTextMatches(text, query, caseSensitive: caseSensitive)
      : aleraSearchTextMatches(text, query, caseSensitive: caseSensitive);
  if (matches.isEmpty) {
    return TextSpan(text: text, style: baseStyle);
  }
  final children = <InlineSpan>[];
  var cursor = 0;
  for (final (index, match) in matches.indexed) {
    if (match.start > cursor) {
      children.add(
        TextSpan(text: text.substring(cursor, match.start), style: baseStyle),
      );
    }
    children.add(
      TextSpan(
        text: text.substring(match.start, match.end),
        style: aleraSearchMatchStyle(
          baseStyle,
          active: index == activeMatchIndex,
        ),
      ),
    );
    cursor = match.end;
  }
  if (cursor < text.length) {
    children.add(TextSpan(text: text.substring(cursor), style: baseStyle));
  }
  return TextSpan(children: children);
}

class const AleraSearchHighlightedText({
  super.key,
  required final String text,
  required final String query,
  final TextStyle? style,
  final bool caseSensitive = false,
  final bool fuzzy = false,
  final int activeMatchIndex = -1,
  final int? maxLines,
  final TextOverflow overflow = TextOverflow.clip,
  final TextAlign textAlign = TextAlign.start,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final baseStyle = style ?? DefaultTextStyle.of(context).style;
    return Text.rich(
      aleraSearchHighlightedSpan(
        text: text,
        query: query,
        baseStyle: baseStyle,
        caseSensitive: caseSensitive,
        fuzzy: fuzzy,
        activeMatchIndex: activeMatchIndex,
      ),
      maxLines: maxLines,
      overflow: overflow,
      textAlign: textAlign,
    );
  }
}
