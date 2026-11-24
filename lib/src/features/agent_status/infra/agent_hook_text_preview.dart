import 'dart:math' as math;

/// Preserves the Hook preview's existing trim and UTF-16 truncation rules.
/// Only normalize enough text to fill the preview: tool outputs may be much
/// larger than the 160/200/8000 code units actually displayed.
String normalizeAgentHookSingleLine(String value, int maxLength) =>
    _normalizePreview(value, maxLength, multiline: false);

String normalizeAgentHookMultiline(String value, int maxLength) =>
    _normalizePreview(value, maxLength, multiline: true);

bool _isLineBreak(int code) =>
    code == 0x0d || code == 0x0a || code == 0x2028 || code == 0x2029;

String _normalizePreview(
  String value,
  int maxLength, {
  required bool multiline,
}) {
  assert(maxLength >= 0);
  if (maxLength == 0) return '';
  // Keep Dart's exact Unicode whitespace semantics instead of approximating
  // trim with a separate whitespace table.
  final text = value.trim();
  final prefixEnd = math.min(text.length, maxLength);
  var index = 0;
  while (index < prefixEnd && !_isLineBreak(text.codeUnitAt(index))) {
    index++;
  }
  // Most prompts and tool names contain no line break in the visible prefix.
  // No intermediate normalized string or StringBuffer is needed.
  if (index == prefixEnd) {
    return index == text.length ? text : text.substring(0, index);
  }

  final output = StringBuffer(text.substring(0, index));
  var remaining = maxLength - index;
  final maxBreaks = multiline ? 2 : 1;
  final separator = multiline ? '\n' : ' ';
  while (index < text.length && remaining > 0) {
    final code = text.codeUnitAt(index);
    if (_isLineBreak(code)) {
      // In multiline previews CRLF is one newline and a consecutive run
      // collapses to at most two. Single-line previews collapse the whole
      // run (including Unicode line separators) to one space.
      var emitted = 0;
      do {
        if (multiline &&
            text.codeUnitAt(index) == 0x0d &&
            index + 1 < text.length &&
            text.codeUnitAt(index + 1) == 0x0a) {
          index++;
        }
        index++;
        if (emitted < maxBreaks) {
          output.write(separator);
          emitted++;
          remaining--;
        }
      } while (remaining > 0 &&
          index < text.length &&
          _isLineBreak(text.codeUnitAt(index)));
    } else {
      final start = index;
      final end = math.min(text.length, index + remaining);
      do {
        index++;
      } while (index < end && !_isLineBreak(text.codeUnitAt(index)));
      output.write(text.substring(start, index));
      remaining -= index - start;
    }
  }
  return output.toString();
}
