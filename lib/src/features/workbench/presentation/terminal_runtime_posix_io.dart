part of 'terminal_runtime.dart';

typedef _ReadNative = ffi.IntPtr Function(
  ffi.Int32 fd,
  ffi.Pointer<ffi.Uint8> buf,
  ffi.UintPtr nbyte,
);
typedef _ReadDart = int Function(int fd, ffi.Pointer<ffi.Uint8> buf, int nbyte);

typedef _ErrnoLocationNative = ffi.Pointer<ffi.Int32> Function();
typedef _ErrnoLocationDart = ffi.Pointer<ffi.Int32> Function();

final ffi.DynamicLibrary _libc = .process();
final _ReadDart _posixRead = _libc.lookupFunction<_ReadNative, _ReadDart>(
  'read',
);

int _currentErrno() {
  final symbol = (Platform.isMacOS || Platform.isIOS)
      ? '__error'
      : '__errno_location';
  return _libc
      .lookupFunction<_ErrnoLocationNative, _ErrnoLocationDart>(symbol)()
      .value;
}

@visibleForTesting
int currentErrnoForTesting() {
  return _currentErrno();
}

const int _eintr = 4;
const int _readChunkSize = 4096;

final class _PosixPtyDecodedTextSink implements Sink<String> {
  _PosixPtyDecodedTextSink(this._sendPort);

  final SendPort _sendPort;

  @override
  void add(String data) {
    if (data.isNotEmpty) {
      _sendPort.send(data);
    }
  }

  @override
  void close() {}
}

void _posixPtyReadIsolate(List<Object?> args) {
  final fd = args[0]! as int;
  final sendPort = args[1]! as SendPort;
  _runPosixPtyReadIsolate(fd: fd, sendPort: sendPort, read: _posixRead);
}

void _runPosixPtyReadIsolate({
  required int fd,
  required SendPort sendPort,
  required int Function(int, ffi.Pointer<ffi.Uint8>, int) read,
}) {
  if (fd < 0) {
    sendPort.send(const <Object?, Object?>{
      'type': 'error',
      'error': 'PTY master file descriptor is unavailable.',
    });
    return;
  }
  final buffer = calloc<ffi.Uint8>(_readChunkSize);
  final decoder = const Utf8Decoder(allowMalformed: true)
      .startChunkedConversion(_PosixPtyDecodedTextSink(sendPort));
  try {
    _readPosixPtyLoop(fd: fd, buffer: buffer, read: read, onBytes: decoder.add);
    // Close before publishing done so an incomplete final sequence is emitted
    // with the same replacement semantics as the UI-isolate decoder used to
    // provide, and so the final text always precedes the EOF marker.
    decoder.close();
    sendPort.send(const <Object?, Object?>{'type': 'done'});
  } catch (error) {
    sendPort.send(<Object?, Object?>{
      'type': 'error',
      'error': error.toString(),
    });
  } finally {
    calloc.free(buffer);
  }
}

void _readPosixPtyLoop({
  required int fd,
  required ffi.Pointer<ffi.Uint8> buffer,
  required int Function(int, ffi.Pointer<ffi.Uint8>, int) read,
  required void Function(List<int> bytes) onBytes,
}) {
  while (true) {
    final byteCount = read(fd, buffer, _readChunkSize);
    if (byteCount > 0) {
      // The decoder consumes this native-buffer view synchronously in the
      // reader isolate, so there is no need to allocate/copy a Uint8List just
      // to transfer it to the UI isolate.
      onBytes(buffer.asTypedList(byteCount));
      continue;
    }
    if (byteCount < 0 && _currentErrno() == _eintr) {
      continue;
    }
    break;
  }
}

@visibleForTesting
void runPosixPtyReadIsolateForTesting({
  required int fd,
  required SendPort sendPort,
  required int Function(int, ffi.Pointer<ffi.Uint8>, int) read,
}) {
  _runPosixPtyReadIsolate(fd: fd, sendPort: sendPort, read: read);
}

bool _writePtyBytes({
  required List<int> bytes,
  required int Function(Uint8List bytes) write,
  required StreamController<TerminalPtySessionEvent> events,
}) {
  try {
    return write(.fromList(bytes)) > 0;
  } catch (error) {
    events.add(TerminalPtyErrorEvent(error));
    return false;
  }
}

void _resizePty({
  required int rows,
  required int cols,
  required void Function({required int rows, required int cols}) resize,
  required StreamController<TerminalPtySessionEvent> events,
}) {
  try {
    resize(rows: rows, cols: cols);
  } catch (error) {
    events.add(TerminalPtyErrorEvent(error));
  }
}

@visibleForTesting
bool writePtyBytesForTesting({
  required List<int> bytes,
  required int Function(Uint8List bytes) write,
  required StreamController<TerminalPtySessionEvent> events,
}) {
  return _writePtyBytes(bytes: bytes, write: write, events: events);
}

@visibleForTesting
void resizePtyForTesting({
  required int rows,
  required int cols,
  required void Function({required int rows, required int cols}) resize,
  required StreamController<TerminalPtySessionEvent> events,
}) {
  _resizePty(rows: rows, cols: cols, resize: resize, events: events);
}

@visibleForTesting
void posixPtyReadIsolateForTesting(List<Object?> args) {
  _posixPtyReadIsolate(args);
}
