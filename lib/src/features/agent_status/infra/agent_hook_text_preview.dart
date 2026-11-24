import 'dart:convert';
import 'dart:math' as math;

/// Preserves the Hook preview's existing trim and UTF-16 truncation rules.
/// Only normalize enough text to fill the preview: tool outputs may be much
/// larger than the 160/200/8000 code units actually displayed.
String normalizeAgentHookSingleLine(String value, int maxLength) =>
    _normalizePreview(value, maxLength, multiline: false);

String normalizeAgentHookMultiline(String value, int maxLength) =>
    _normalizePreview(value, maxLength, multiline: true);

/// Format the fallback preview directly from the SDK JSON encoder. Keep the
/// same escaping, map order, custom toJson and error semantics as jsonEncode,
/// including validation of objects after the preview is full. Only the
/// retained output is bounded; the encoder still traverses the input.
String? normalizeAgentHookJsonPreview(Object? value, int maxLength) {
  assert(maxLength >= 0);
  final output = _JsonPreviewSink(maxLength);
  try {
    final encoder = const JsonEncoder().startChunkedConversion(
      StringConversionSink.fromStringSink(output),
    );
    encoder.add(value);
    encoder.close();
    return output.result;
  } catch (_) {
    return null;
  }
}

// Compact JSON starts and ends with a non-whitespace token. Escaped CR/LF
// remain literal backslash sequences, while raw Unicode line separators are
// normalized just as in the single-line string preview. Carry the line-break
// state across encoder writes rather than normalizing each chunk separately.
final class _JsonPreviewSink implements StringSink {
  _JsonPreviewSink(this._limit);

  final int _limit;
  final StringBuffer _buffer = StringBuffer();
  bool _inBreak = false;

  String get result => _buffer.toString();
  bool get _full => _buffer.length >= _limit;

  @override
  void write(Object? object) {
    if (_full) return;
    final text = object.toString();
    for (var index = 0; index < text.length && !_full; index++) {
      _writeUnit(text.codeUnitAt(index));
    }
  }

  void _writeUnit(int code) {
    if (_isLineBreak(code)) {
      if (!_inBreak) _buffer.writeCharCode(0x20);
      _inBreak = true;
    } else {
      _buffer.writeCharCode(code);
      _inBreak = false;
    }
  }

  @override
  void writeCharCode(int charCode) {
    if (_full) return;
    if (charCode <= 0xffff) {
      _writeUnit(charCode);
    } else {
      write(String.fromCharCode(charCode));
    }
  }

  @override
  void writeAll(Iterable<Object?> objects, [String separator = '']) {
    var first = true;
    for (final object in objects) {
      if (_full) return;
      if (!first) write(separator);
      first = false;
      write(object);
    }
  }

  @override
  void writeln([Object? object = '']) {
    write(object);
    writeCharCode(0x0a);
  }
}

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
