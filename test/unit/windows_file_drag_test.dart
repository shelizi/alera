import 'dart:io';

import 'package:alera/src/features/workbench/application/windows_file_drag.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('alera.test/native_drag');
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;

  tearDown(() => messenger.setMockMethodCallHandler(channel, null));

  test('no paths means no OS drag', () async {
    var called = false;
    messenger.setMockMethodCallHandler(channel, (_) async {
      called = true;
      return 'move';
    });
    final drag = WindowsFileDrag(channel: channel);
    expect(await drag.startDrag(const []), WindowsFileDragEffect.cancelled);
    expect(called, isFalse);
  });

  test(
    'selected paths and copy effect round-trip through MethodChannel',
    () async {
      if (!Platform.isWindows) return;
      MethodCall? seen;
      messenger.setMockMethodCallHandler(channel, (call) async {
        seen = call;
        return 'copy';
      });
      final drag = WindowsFileDrag(channel: channel);
      expect(
        await drag.startDrag(const ['first.txt', 'second.txt']),
        WindowsFileDragEffect.copy,
      );
      expect(seen?.method, 'startDrag');
      expect(seen?.arguments, {
        'paths': ['first.txt', 'second.txt'],
      });
    },
  );

  test('Windows drop effect maps to move or cancel', () async {
    if (!Platform.isWindows) return;
    final drag = WindowsFileDrag(channel: channel);
    messenger.setMockMethodCallHandler(channel, (_) async => 'move');
    expect(await drag.startDrag(const ['one.txt']), WindowsFileDragEffect.move);
    messenger.setMockMethodCallHandler(channel, (_) async => 'cancelled');
    expect(
      await drag.startDrag(const ['one.txt']),
      WindowsFileDragEffect.cancelled,
    );
  });

  test('native cursor check asks OS window hit test', () async {
    if (!Platform.isWindows) return;
    messenger.setMockMethodCallHandler(channel, (call) async {
      expect(call.method, 'cursorOutsideWindow');
      return true;
    });
    expect(
      await WindowsFileDrag(channel: channel).cursorOutsideWindow(),
      isTrue,
    );
  });
}
