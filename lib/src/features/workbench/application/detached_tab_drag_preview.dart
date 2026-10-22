import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Flutter's Draggable avatar is clipped at the native HWND. Windows displays
/// a click-through, native drag preview outside that HWND instead.
class DetachedTabDragPreview {
  const DetachedTabDragPreview({
    this.channel = const MethodChannel('dev.leynier.alera/detached_tabs'),
    bool? enabled,
  }) : _enabledOverride = enabled;

  final MethodChannel channel;
  final bool? _enabledOverride;

  bool get _enabled =>
      _enabledOverride ?? defaultTargetPlatform == TargetPlatform.windows;

  Future<void> start(String title) async {
    if (!_enabled) return;
    try {
      await channel.invokeMethod<void>('startDragPreview', {'title': title});
    } on MissingPluginException {
      // Older runners and widget tests have no native preview channel.
    } on PlatformException {
      // Native preview is best-effort; Flutter's in-window drag still works.
    }
  }

  Future<void> stop() async {
    if (!_enabled) return;
    try {
      await channel.invokeMethod<void>('stopDragPreview');
    } on MissingPluginException {
    } on PlatformException {}
  }
}

const detachedTabDragPreview = DetachedTabDragPreview();
