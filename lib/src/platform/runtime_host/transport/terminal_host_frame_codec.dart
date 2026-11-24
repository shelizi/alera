/// Length-prefixed framing for the local terminal-host socket.
///
/// Mirrors `rust/alera-cli/src/terminal_host/frame_codec.rs`. Deliberately free
/// of Flutter imports so it can also run inside a plain isolate.
///
/// ```text
/// [u8 kind][u32be length][payload]
///
/// kind 1 = JSON     payload = the same object that would have been a line
/// kind 2 = output   payload = [u16be len][sessionId utf8][raw pty bytes]
/// ```
library;

import 'dart:convert';
import 'dart:typed_data';

const int terminalHostFrameKindJson = 1;
const int terminalHostFrameKindOutput = 2;

/// Kind byte plus the u32 length.
const int terminalHostFrameHeaderLength = 5;

/// Bounds both old-style JSON snapshot replies (10 MiB of raw scrollback,
/// base64-encoded) and newer binary frames. An invalid advertised length must
/// not make the reader wait forever or retain an unbounded input buffer.
const int terminalHostMaxFramePayloadBytes = 32 * 1024 * 1024;

/// Last line before the stream switches to frames.
///
/// The switch has to be visible in the bytes: the reader can live in another
/// isolate, so anything that flips it from outside races with the port
/// round-trip and mis-parses whatever arrived in between.
const String terminalHostBinaryFramesEnabledLine = 'binaryFramesEnabled';

sealed class const TerminalHostFrame();

final class const TerminalHostJsonFrame(final String json)
    extends TerminalHostFrame;

final class const TerminalHostOutputFrame(
  final String sessionId,
  final Uint8List data,
) extends TerminalHostFrame;

/// Incremental reader over a byte stream that may switch mid-connection.
///
/// The connection starts newline-delimited so the `hello` handshake works
/// against a host that does not support frames. Once the response confirms the
/// upgrade, [upgradeToBinary] is called and every later byte is framed. The
/// switch is driven from here, by the code that owns the bytes, so there is no
/// window where the mode is ambiguous.
class TerminalHostFrameReader {
  // Complete frames are read directly from the socket's typed chunk. Only
  // unfinished payloads are copied into owned storage, which grows amortized
  // rather than shifting a boxed List<int> after every decoded frame.
  final BytesBuilder _partial = BytesBuilder(copy: true);
  final Uint8List _header = Uint8List(terminalHostFrameHeaderLength);
  int _headerLength = 0;
  int _kind = 0;
  int? _remainingPayload;
  bool _binary = false;

  bool get isBinary => _binary;

  /// The out-of-band test/compatibility hook is valid at a frame boundary.
  /// The actual socket switches in band after consuming the complete sentinel.
  void upgradeToBinary() {
    if (_binary) return;
    if (_partial.isNotEmpty) {
      throw const FormatException(
        'Binary upgrade inside a terminal host line.',
      );
    }
    _binary = true;
  }

  /// Appends [chunk] and returns whatever frames are now complete.
  ///
  /// Neither retained fragments nor returned output alias the caller's mutable
  /// input. JSON and session IDs decode from ranges without allocating slices.
  List<TerminalHostFrame> add(List<int> chunk) {
    final bytes = chunk is Uint8List ? chunk : Uint8List.fromList(chunk);
    final frames = <TerminalHostFrame>[];
    var offset = 0;
    while (offset < bytes.length) {
      if (!_binary) {
        // Each incoming fragment is searched exactly once, even for a large
        // legacy snapshot response split into thousands of socket reads.
        final newline = bytes.indexOf(0x0a, offset);
        final end = newline < 0 ? bytes.length : newline;
        if (_partial.length + end - offset > terminalHostMaxFramePayloadBytes) {
          throw const FormatException('Terminal host line exceeds size limit.');
        }
        if (newline < 0) {
          _appendPartial(bytes, offset, end);
          break;
        }
        final String line;
        if (_partial.isEmpty) {
          line = _decodeUtf8(bytes, offset, end);
        } else {
          _appendPartial(bytes, offset, end);
          final assembled = _partial.takeBytes();
          line = _decodeUtf8(assembled, 0, assembled.length);
        }
        frames.add(TerminalHostJsonFrame(line));
        offset = newline + 1;
        if (_isUpgradeLine(line)) {
          upgradeToBinary();
        }
        continue;
      }

      if (_remainingPayload == null) {
        if (_headerLength == 0 &&
            bytes.length - offset >= terminalHostFrameHeaderLength) {
          // The normal path does not copy the header either.
          _readHeader(bytes, offset);
          offset += terminalHostFrameHeaderLength;
        } else {
          final missing = terminalHostFrameHeaderLength - _headerLength;
          final available = bytes.length - offset;
          final take = missing < available ? missing : available;
          _header.setRange(_headerLength, _headerLength + take, bytes, offset);
          _headerLength += take;
          offset += take;
          if (_headerLength < terminalHostFrameHeaderLength) {
            break;
          }
          _readHeader(_header, 0);
          _headerLength = 0;
        }
      }

      final remaining = _remainingPayload!;
      final available = bytes.length - offset;
      final take = remaining < available ? remaining : available;
      final end = offset + take;
      final known =
          _kind == terminalHostFrameKindJson ||
          _kind == terminalHostFrameKindOutput;
      if (take < remaining) {
        // Never allocate the advertised frame size just because a header was
        // received. Unknown kinds can be discarded without retaining payloads.
        if (known) _appendPartial(bytes, offset, end);
        _remainingPayload = remaining - take;
        break;
      }

      if (known) {
        final TerminalHostFrame? frame;
        if (_partial.isEmpty) {
          frame = _decodeFrame(_kind, bytes, offset, end);
        } else {
          _appendPartial(bytes, offset, end);
          final assembled = _partial.takeBytes();
          frame = _decodeFrame(_kind, assembled, 0, assembled.length);
        }
        if (frame != null) frames.add(frame);
      }
      _remainingPayload = null;
      offset = end;
      // Unknown kinds and malformed output payloads consume their advertised
      // bytes, not the rest of this socket read. Continue with the next frame.
    }
    return frames;
  }

  void _readHeader(Uint8List bytes, int start) {
    final length =
        (bytes[start + 1] << 24) |
        (bytes[start + 2] << 16) |
        (bytes[start + 3] << 8) |
        bytes[start + 4];
    if (length > terminalHostMaxFramePayloadBytes) {
      throw const FormatException('Terminal host frame exceeds size limit.');
    }
    _kind = bytes[start];
    _remainingPayload = length;
  }

  void _appendPartial(Uint8List bytes, int start, int end) {
    if (start < end) {
      _partial.add(Uint8List.sublistView(bytes, start, end));
    }
  }

  static String _decodeUtf8(Uint8List bytes, int start, int end) =>
      const Utf8Decoder(allowMalformed: true).convert(bytes, start, end);

  static bool _isUpgradeLine(String line) {
    // Avoid parsing ordinary lines twice. A mention in a message body is not
    // the upgrade event and must not flip the transport into binary mode.
    if (!line.contains(terminalHostBinaryFramesEnabledLine)) return false;
    try {
      final decoded = jsonDecode(line);
      return decoded is Map &&
          decoded['event'] == terminalHostBinaryFramesEnabledLine;
    } on FormatException {
      return false;
    }
  }

  static TerminalHostFrame? _decodeFrame(
    int kind,
    Uint8List bytes,
    int start,
    int end,
  ) {
    if (kind == terminalHostFrameKindJson) {
      return TerminalHostJsonFrame(_decodeUtf8(bytes, start, end));
    }
    if (end - start < 2) return null;
    final idLength = (bytes[start] << 8) | bytes[start + 1];
    final idEnd = start + 2 + idLength;
    if (idEnd > end) return null;
    final sessionId = _decodeUtf8(bytes, start + 2, idEnd);
    // Exactly one owned output copy, even when the input chunk contains many
    // frames. A small output cannot pin an entire large socket buffer.
    return TerminalHostOutputFrame(sessionId, bytes.sublist(idEnd, end));
  }
}

/// Encodes a JSON frame, used by tests and by any writer that speaks frames.
Uint8List encodeTerminalHostJsonFrame(String json) {
  final payload = utf8.encode(json);
  return _frame(terminalHostFrameKindJson, payload);
}

Uint8List encodeTerminalHostOutputFrame(String sessionId, List<int> data) {
  final id = utf8.encode(sessionId);
  final payloadLength = 2 + id.length + data.length;
  final out = _frameBuffer(terminalHostFrameKindOutput, payloadLength);
  const start = terminalHostFrameHeaderLength;
  out[start] = (id.length >> 8) & 0xff;
  out[start + 1] = id.length & 0xff;
  final idEnd = start + 2 + id.length;
  out.setRange(start + 2, idEnd, id);
  out.setRange(idEnd, out.length, data);
  return out;
}

Uint8List _frame(int kind, List<int> payload) =>
    _frameBuffer(kind, payload.length)..setRange(
      terminalHostFrameHeaderLength,
      terminalHostFrameHeaderLength + payload.length,
      payload,
    );

Uint8List _frameBuffer(int kind, int payloadLength) =>
    Uint8List(terminalHostFrameHeaderLength + payloadLength)
      ..[0] = kind
      ..[1] = (payloadLength >> 24) & 0xff
      ..[2] = (payloadLength >> 16) & 0xff
      ..[3] = (payloadLength >> 8) & 0xff
      ..[4] = payloadLength & 0xff;
