import 'dart:io';

import 'package:flutter/services.dart';

/// Hands a Flutter Explorer drag gesture over to the Windows Shell.
/// The Shell owns both the destination and the resulting filesystem operation.
enum WindowsFileDragEffect { cancelled, copy, move, none }

class WindowsFileDrag {
  const WindowsFileDrag({
    this.channel = const MethodChannel('dev.leynier.alera/native_file_drag'),
  });

  final MethodChannel channel;

  Future<bool> cursorOutsideWindow() async {
    if (!Platform.isWindows) return false;
    return await channel.invokeMethod<bool>('cursorOutsideWindow') ?? false;
  }

  Future<WindowsFileDragEffect> startDrag(List<String> absolutePaths) async {
    if (!Platform.isWindows || absolutePaths.isEmpty) {
      return WindowsFileDragEffect.cancelled;
    }
    final result = await channel.invokeMethod<String>(
      'startDrag',
      <String, Object?>{'paths': absolutePaths},
    );
    return switch (result) {
      'copy' => WindowsFileDragEffect.copy,
      'move' => WindowsFileDragEffect.move,
      'none' => WindowsFileDragEffect.none,
      _ => WindowsFileDragEffect.cancelled,
    };
  }
}
