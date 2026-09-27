import 'dart:convert';
import 'dart:isolate';
import 'dart:typed_data';

/// Bodies at or below this size decode on the calling isolate; larger ones
/// (completion lists, workspace diagnostics, semantic tokens) decode in a
/// short-lived isolate so a multi-megabyte payload cannot stall UI frames.
const int lspInlineDecodeLimitBytes = 64 * 1024;

/// Splits a Language Server Protocol stdio byte stream into message bodies.
///
/// Bytes are kept in one growable [Uint8List] with read and write offsets, so
/// a large message arriving in many chunks costs one copy per chunk rather
/// than a list rebuild per message.
final class LspMessageFramer {
  static const int _initialCapacity = 64 * 1024;

  Uint8List _buffer = Uint8List(_initialCapacity);
  int _start = 0;
  int _end = 0;
  int _scanFrom = 0;
  int? _pendingBodyStart;
  int _pendingBodyLength = 0;

  /// Appends [chunk] and returns every message body it completed, in order.
  List<Uint8List> add(List<int> chunk) {
    _append(chunk);
    final bodies = <Uint8List>[];
    while (true) {
      var bodyStart = _pendingBodyStart;
      if (bodyStart == null) {
        final headerEnd = _findHeaderEnd();
        if (headerEnd == -1) break;
        bodyStart = headerEnd + 4;
        _pendingBodyStart = bodyStart;
        _pendingBodyLength = _contentLength(_start, headerEnd);
      }
      final bodyEnd = bodyStart + _pendingBodyLength;
      if (bodyEnd > _end) break;
      bodies.add(
        Uint8List.fromList(Uint8List.sublistView(_buffer, bodyStart, bodyEnd)),
      );
      _start = bodyEnd;
      _scanFrom = bodyEnd;
      _pendingBodyStart = null;
    }
    if (_start == _end) {
      _start = 0;
      _end = 0;
      _scanFrom = 0;
    }
    return bodies;
  }

  void _append(List<int> chunk) {
    final needed = _end + chunk.length;
    if (needed > _buffer.length) {
      final live = _end - _start;
      var capacity = _buffer.length;
      while (capacity < live + chunk.length) {
        capacity *= 2;
      }
      final next = capacity == _buffer.length ? _buffer : Uint8List(capacity);
      next.setRange(0, live, _buffer, _start);
      final shift = _start;
      _buffer = next;
      _start = 0;
      _end = live;
      _scanFrom -= shift;
      final pending = _pendingBodyStart;
      if (pending != null) _pendingBodyStart = pending - shift;
    }
    _buffer.setRange(_end, _end + chunk.length, chunk);
    _end += chunk.length;
  }

  int _findHeaderEnd() {
    for (var i = _scanFrom; i <= _end - 4; i++) {
      if (_buffer[i] == 13 &&
          _buffer[i + 1] == 10 &&
          _buffer[i + 2] == 13 &&
          _buffer[i + 3] == 10) {
        return i;
      }
    }
    // The terminator may straddle this chunk and the next one.
    _scanFrom = _end - 3 > _start ? _end - 3 : _start;
    return -1;
  }

  int _contentLength(int headerStart, int headerEnd) {
    final header = latin1.decode(
      Uint8List.sublistView(_buffer, headerStart, headerEnd),
    );
    final match = RegExp(r'Content-Length: (\d+)').firstMatch(header);
    return int.parse(match?.group(1) ?? '0');
  }
}

/// Decodes one message body, off the calling isolate when it is large.
Future<Object?> decodeLspMessageBody(Uint8List body) {
  if (body.length <= lspInlineDecodeLimitBytes) {
    return Future<Object?>.sync(() => _decodeBody(body));
  }
  return Isolate.run(() => _decodeBody(body));
}

Object? _decodeBody(Uint8List body) => jsonDecode(utf8.decode(body));
