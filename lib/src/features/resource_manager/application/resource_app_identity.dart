import 'dart:ffi' as ffi;
import 'dart:io' show Platform, pid;

import 'package:ffi/ffi.dart' show calloc;

final class _Filetime extends ffi.Struct {
  @ffi.Uint32()
  external int low;

  @ffi.Uint32()
  external int high;
}

typedef _GetProcessTimesNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<_Filetime>,
  ffi.Pointer<_Filetime>,
  ffi.Pointer<_Filetime>,
  ffi.Pointer<_Filetime>,
);
typedef _GetProcessTimesDart = int Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<_Filetime>,
  ffi.Pointer<_Filetime>,
  ffi.Pointer<_Filetime>,
  ffi.Pointer<_Filetime>,
);

// A process's creation time never changes. Read it once, lazily, from the
// process's own Win32 pseudohandle; no PID lookup or helper process required.
final String? _ownCreationFiletime = _readOwnCreationFiletime();

/// Full Windows creation marker for this caller only, or null when
/// unavailable. Use a decimal string to retain every FILETIME bit in JSON.
String? resourceAppCreationFiletime(int appPid) =>
    appPid == pid ? _ownCreationFiletime : null;

String? _readOwnCreationFiletime() {
  if (!Platform.isWindows) return null;
  try {
    final kernel32 = ffi.DynamicLibrary.open('kernel32.dll');
    final getCurrentProcess = kernel32
        .lookupFunction<
          ffi.Pointer<ffi.Void> Function(),
          ffi.Pointer<ffi.Void> Function()
        >('GetCurrentProcess');
    final getProcessTimes = kernel32
        .lookupFunction<_GetProcessTimesNative, _GetProcessTimesDart>(
          'GetProcessTimes',
        );
    final times = calloc<_Filetime>(4);
    try {
      if (getProcessTimes(
            getCurrentProcess(),
            times,
            times + 1,
            times + 2,
            times + 3,
          ) ==
          0) {
        return null;
      }
      final creation = (times.ref.high << 32) | times.ref.low;
      return creation >= 116444736010000000 ? creation.toString() : null;
    } finally {
      calloc.free(times);
    }
  } catch (_) {
    // The resource monitor is ambient UI. Missing Win32 support must leave
    // the backwards-compatible bare-PID path available.
    return null;
  }
}
