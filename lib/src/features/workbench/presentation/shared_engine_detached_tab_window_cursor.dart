import 'dart:ffi' as ffi;
import 'dart:io';
import 'dart:ui' show Rect;

import 'package:ffi/ffi.dart' show calloc;

/// Native cursor hit test against the shared detached windows. `viewId` is
/// the detached view under the pointer, or null when the pointer is on the
/// primary window or another Alera surface; `onForeignWindow` marks a hit on
/// a window owned by a different process so callers stop instead of docking.
typedef SharedCursorHit = ({int? viewId, bool onForeignWindow});

final class _Win32Point extends ffi.Struct {
  @ffi.Int32()
  external int x;

  @ffi.Int32()
  external int y;
}

final class _Win32Rect extends ffi.Struct {
  @ffi.Int32()
  external int left;

  @ffi.Int32()
  external int top;

  @ffi.Int32()
  external int right;

  @ffi.Int32()
  external int bottom;
}

final ffi.DynamicLibrary _user32 = ffi.DynamicLibrary.open('user32.dll');
final ffi.DynamicLibrary _kernel32 = ffi.DynamicLibrary.open('kernel32.dll');

final _getCursorPos = _user32
    .lookupFunction<
      ffi.Int32 Function(ffi.Pointer<_Win32Point>),
      int Function(ffi.Pointer<_Win32Point>)
    >('GetCursorPos');
final _windowFromPoint = _user32
    .lookupFunction<
      ffi.Pointer<ffi.Void> Function(_Win32Point),
      ffi.Pointer<ffi.Void> Function(_Win32Point)
    >('WindowFromPoint');
final _getAncestor = _user32
    .lookupFunction<
      ffi.Pointer<ffi.Void> Function(ffi.Pointer<ffi.Void>, ffi.Uint32),
      ffi.Pointer<ffi.Void> Function(ffi.Pointer<ffi.Void>, int)
    >('GetAncestor');
final _getWindowThreadProcessId = _user32
    .lookupFunction<
      ffi.Uint32 Function(ffi.Pointer<ffi.Void>, ffi.Pointer<ffi.Uint32>),
      int Function(ffi.Pointer<ffi.Void>, ffi.Pointer<ffi.Uint32>)
    >('GetWindowThreadProcessId');
final _getWindowRect = _user32
    .lookupFunction<
      ffi.Int32 Function(ffi.Pointer<ffi.Void>, ffi.Pointer<_Win32Rect>),
      int Function(ffi.Pointer<ffi.Void>, ffi.Pointer<_Win32Rect>)
    >('GetWindowRect');
final _getCurrentProcessId = _kernel32
    .lookupFunction<ffi.Uint32 Function(), int Function()>(
      'GetCurrentProcessId',
    );

const int _gaRoot = 2;
const double _rectTolerance = 2;

/// Identifies the detached window under the OS cursor by matching the hit
/// HWND's root `GetWindowRect` against [viewBounds] (both physical pixels).
/// Matching on the HWND rectangle, not a point-in-rect sweep, keeps layered
/// or overlapping windows from shadowing each other's drop target.
SharedCursorHit? detachedWindowUnderCursor(Map<int, Rect> viewBounds) {
  if (!Platform.isWindows) return null;
  final point = calloc<_Win32Point>();
  final pid = calloc<ffi.Uint32>();
  final rect = calloc<_Win32Rect>();
  try {
    if (_getCursorPos(point) == 0) return null;
    var hwnd = _windowFromPoint(point.ref);
    if (hwnd == ffi.nullptr) return null;
    final root = _getAncestor(hwnd, _gaRoot);
    if (root != ffi.nullptr) hwnd = root;
    _getWindowThreadProcessId(hwnd, pid);
    if (pid.value != _getCurrentProcessId()) {
      return (viewId: null, onForeignWindow: true);
    }
    if (_getWindowRect(hwnd, rect) == 0) {
      return (viewId: null, onForeignWindow: false);
    }
    for (final entry in viewBounds.entries) {
      final bounds = entry.value;
      if ((bounds.left - rect.ref.left).abs() <= _rectTolerance &&
          (bounds.top - rect.ref.top).abs() <= _rectTolerance &&
          (bounds.right - rect.ref.right).abs() <= _rectTolerance &&
          (bounds.bottom - rect.ref.bottom).abs() <= _rectTolerance) {
        return (viewId: entry.key, onForeignWindow: false);
      }
    }
    return (viewId: null, onForeignWindow: false);
  } catch (_) {
    return null;
  } finally {
    calloc.free(point);
    calloc.free(pid);
    calloc.free(rect);
  }
}
