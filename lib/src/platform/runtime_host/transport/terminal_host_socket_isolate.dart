/// Socket reader that runs off the UI isolate.
///
/// It owns the TCP socket, does the framing, and decodes PTY output to text so
/// the main isolate receives something it can hand straight to the emulator.
/// The per-session decoder has to live here: a multi-byte sequence can be split
/// across chunks, and this is the only place that sees every chunk in order.
///
/// No Flutter imports: this runs in a plain isolate.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:isolate';
import 'dart:typed_data';

import 'package:alera/src/platform/runtime_host/transport/terminal_host_frame_codec.dart';

/// Messages the isolate sends to the main isolate.
const String terminalHostIsolateReady = 'ready';
const String terminalHostIsolateLine = 'line';
const String terminalHostIsolateOutput = 'output';
const String terminalHostIsolateOutputBatch = 'outputBatch';
const String terminalHostIsolateClosed = 'closed';
const String terminalHostIsolateError = 'error';

/// Commands the main isolate sends back.
const String terminalHostIsolateWrite = 'write';
const String terminalHostIsolateClose = 'close';

class const TerminalHostSocketIsolateConfig({
  required final String host,
  required final int port,
  required final SendPort toMain,
  required final int connectTimeoutMillis,
});

/// Isolate entry point. Sends [terminalHostIsolateReady] with its command port
/// once connected, then streams frames until the socket or the owner closes it.
Future<void> terminalHostSocketIsolateMain(
  TerminalHostSocketIsolateConfig config,
) async {
  final toMain = config.toMain;
  final Socket socket;
  try {
    socket = await Socket.connect(
      config.host,
      config.port,
      timeout: Duration(milliseconds: config.connectTimeoutMillis),
    );
  } catch (_) {
    // Error categories are intentionally bounded: socket exception text can
    // contain machine-specific details and should not enter crash reports.
    toMain.send(const <Object?>[terminalHostIsolateError, 'connect']);
    toMain.send(const <Object?>[terminalHostIsolateClosed]);
    return;
  }
  socket.setOption(.tcpNoDelay, true);

  final commands = ReceivePort();
  final reader = TerminalHostFrameReader();
  final dispatcher = TerminalHostSocketFrameDispatcher(toMain.send);
  var closed = false;

  void closeSocket() {
    if (closed) {
      return;
    }
    closed = true;
    socket.destroy();
    commands.close();
  }

  commands.listen((Object? message) {
    if (message is! List || message.isEmpty) {
      return;
    }
    switch (message[0]) {
      case terminalHostIsolateWrite:
        if (!closed) {
          try {
            socket.add(message[1] as List<int>);
          } on Object {
            toMain.send(const <Object?>[terminalHostIsolateError, 'write']);
            closeSocket();
            toMain.send(const <Object?>[terminalHostIsolateClosed]);
          }
        }
      case terminalHostIsolateClose:
        closeSocket();
    }
  });

  socket.listen(
    (Uint8List chunk) {
      if (closed) return;
      try {
        dispatcher.dispatch(reader.add(chunk));
      } on Object {
        toMain.send(const <Object?>[terminalHostIsolateError, 'protocol']);
        closeSocket();
        toMain.send(const <Object?>[terminalHostIsolateClosed]);
      }
    },
    onError: (Object _) {
      toMain.send(const <Object?>[terminalHostIsolateError, 'read']);
      closeSocket();
      toMain.send(const <Object?>[terminalHostIsolateClosed]);
    },
    onDone: () {
      closeSocket();
      toMain.send(const <Object?>[terminalHostIsolateClosed]);
    },
    cancelOnError: true,
  );

  toMain.send(<Object?>[terminalHostIsolateReady, commands.sendPort]);
}

/// Decodes the complete frames from one socket read and sends bounded output
/// envelopes. Keep each logical output event (including its session ID) intact:
/// only the isolate message overhead is shared. Never wait for the next read.
final class TerminalHostSocketFrameDispatcher {
  TerminalHostSocketFrameDispatcher(this._send);

  final void Function(List<Object?>) _send;
  final _decoders = <String, ByteConversionSink>{};
  final _decoded = <String, StringBuffer>{};

  // Keep the UI-side unpack bounded, even for a very large coalesced socket
  // read. An individual oversized frame is still delivered on its own.
  static const int maxBatchEvents = 64;
  static const int maxBatchChars = 128 * 1024;

  void dispatch(List<TerminalHostFrame> frames) {
    List<Object?>? pending;
    var pendingChars = 0;

    void flush() {
      final message = pending;
      if (message == null) return;
      pending = null;
      pendingChars = 0;
      _send(message);
    }

    try {
      for (final frame in frames) {
        switch (frame) {
          case TerminalHostJsonFrame(:final json):
            // A control reply can change the meaning of subsequent PTY
            // bytes (e.g. resync). Keep it as a separate port message and
            // never batch output across it.
            flush();
            _send(<Object?>[terminalHostIsolateLine, decodeHostLine(json)]);
          case TerminalHostOutputFrame(:final sessionId, :final data):
            final text = _decodeOutput(sessionId, data);
            if (text == null) continue;
            final previous = pending;
            if (previous != null &&
                (previous.length >= 1 + 2 * maxBatchEvents ||
                    pendingChars + text.length > maxBatchChars)) {
              flush();
            }
            final message = pending;
            if (message == null) {
              pending = <Object?>[terminalHostIsolateOutput, sessionId, text];
            } else {
              // Flat pairs avoid allocating a second List for every frame.
              message[0] = terminalHostIsolateOutputBatch;
              message
                ..add(sessionId)
                ..add(text);
            }
            pendingChars += text.length;
            if (pendingChars >= maxBatchChars) flush();
        }
      }
    } finally {
      // Preserve already decoded output if a later frame fails, and preserve
      // the usual immediate delivery for a single interactive output frame.
      flush();
    }
  }

  /// Decoder state belongs to each session, not each frame or batch.
  String? _decodeOutput(String sessionId, Uint8List bytes) {
    final buffer = _decoded.putIfAbsent(sessionId, StringBuffer.new);
    final sink = _decoders.putIfAbsent(sessionId, () {
      // withCallback would only deliver on close for a long-lived PTY.
      return const Utf8Decoder(allowMalformed: true)
          .startChunkedConversion(StringConversionSink.fromStringSink(buffer));
    });
    sink.add(bytes);
    if (buffer.isEmpty) return null;
    final text = buffer.toString();
    buffer.clear();
    return text;
  }
}

/// Parses a control line here rather than on the UI isolate.
///
/// An attach reply carries the session's scrollback base64'd inside it, and
/// parsing megabytes of JSON was the single largest main-isolate cost of
/// opening a terminal. A line that will not parse is passed through as-is so
/// the owner still reports it the way it always did.
Object? decodeHostLine(String line) {
  try {
    final decoded = jsonDecode(line);
    _predecodeSnapshotPayload(decoded);
    return decoded;
  } catch (_) {
    return line;
  }
}

/// Attach/resume responses can carry up to the host's full scrollback cap.
/// Keep both expensive transforms -- base64 and UTF-8 decoding -- on the
/// socket isolate rather than handing a multi-megabyte encoded string back to
/// the UI isolate. This changes only the isolate's internal message shape; the
/// host wire protocol remains unchanged.
void _predecodeSnapshotPayload(Object? message) {
  if (message is! Map) {
    return;
  }
  final payload = message['payload'];
  if (payload is! Map) {
    return;
  }
  final encoded = payload['snapshotBase64'];
  if (encoded is! String) {
    return;
  }
  try {
    final bytes = base64Decode(encoded);
    payload['snapshotText'] = const Utf8Decoder(allowMalformed: true)
        .convert(bytes);
    payload.remove('snapshotBase64');
  } catch (_) {
    // Preserve the original payload if it is malformed so the main isolate's
    // existing protocol validation/error path remains authoritative.
  }
}

/// Spawns the reader and returns its command port, or null if spawning failed.
///
/// A failure is not fatal: the caller falls back to reading the socket on the
/// main isolate, because losing terminals over an isolate problem would be a
/// far worse outcome than losing the offload.
Future<SendPort?> spawnTerminalHostSocketIsolate({
  required String host,
  required int port,
  required SendPort toMain,
  required Duration connectTimeout,
  required Future<SendPort> Function() awaitReady,
}) async {
  try {
    await Isolate.spawn<TerminalHostSocketIsolateConfig>(
      terminalHostSocketIsolateMain,
      TerminalHostSocketIsolateConfig(
        host: host,
        port: port,
        toMain: toMain,
        connectTimeoutMillis: connectTimeout.inMilliseconds,
      ),
      debugName: 'alera-terminal-host-socket',
      // Unexpected isolate failures must not strand a connected PTY forever:
      // the owner sees a classified error and can reconnect to the live host.
      onError: toMain,
      onExit: toMain,
    );
  } catch (_) {
    return null;
  }
  return awaitReady();
}
