import 'dart:ui';

import 'package:multiview_desktop/multiview_desktop.dart';

/// Explicit, per-view native window operations.
///
/// Never route secondary view operations through window_manager: that plugin
/// resolves the original engine HWND even when called in a secondary view.
abstract interface class NativeViewHandle {
  int get viewId;
  String getTitle();
  void setTitle(String title);
  Rect getBounds();
  Future<void> setPosition(Offset value);
  Future<bool> setSize(Size value);
  void focus();
  bool isFocused();
  Future<bool> closeWindow();
}

final class MultiviewNativeViewHandle implements NativeViewHandle {
  MultiviewNativeViewHandle(int viewId)
    : viewId = viewId,
      _window = MultiViewDesktop.fromId(viewId);

  @override
  final int viewId;
  final MultiViewDesktop _window;

  @override
  String getTitle() => _window.getTitle();

  @override
  void setTitle(String title) => _window.setTitle(title);

  @override
  Rect getBounds() => _window.getBounds();

  @override
  Future<void> setPosition(Offset value) => _window.setPosition(value);

  @override
  Future<bool> setSize(Size value) => _window.setSize(value);

  @override
  void focus() => _window.focus();

  @override
  bool isFocused() => _window.isFocused();

  @override
  Future<bool> closeWindow() => _window.closeWindow();
}

/// An isolated replacement for global windowManager calls.
///
/// [viewId] is mandatory. On integration, it must come from the calling
/// Flutter View, never the currently-focused HWND or process-global state.
final class ScopedNativeWindowController {
  ScopedNativeWindowController({NativeViewHandle Function(int viewId)? resolve})
    : _resolve = resolve ?? MultiviewNativeViewHandle.new;

  final NativeViewHandle Function(int viewId) _resolve;

  NativeViewHandle forView(int viewId) {
    if (viewId <= 0) {
      throw ArgumentError.value(
        viewId,
        'viewId',
        'Expected a public window ID',
      );
    }
    final resolved = _resolve(viewId);
    if (resolved.viewId != viewId) {
      throw StateError(
        'Resolved view mismatch: requested $viewId, got ${resolved.viewId}',
      );
    }
    return resolved;
  }

  String getTitle(int viewId) => forView(viewId).getTitle();
  void setTitle(int viewId, String title) => forView(viewId).setTitle(title);
  Rect getBounds(int viewId) => forView(viewId).getBounds();

  Future<void> setPosition(int viewId, Offset value) =>
      forView(viewId).setPosition(value);

  Future<bool> setSize(int viewId, Size value) =>
      forView(viewId).setSize(value);

  void focus(int viewId) => forView(viewId).focus();
  bool isFocused(int viewId) => forView(viewId).isFocused();
  Future<bool> close(int viewId) => forView(viewId).closeWindow();
}
