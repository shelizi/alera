import 'dart:ui';

import 'package:alera_multiview_poc/view_window_controller.dart';
import 'package:flutter_test/flutter_test.dart';

class FakeWindowHandle implements NativeViewHandle {
  FakeWindowHandle(this.viewId);
  @override
  final int viewId;
  String title = '';
  bool focused = false;
  bool closed = false;
  Rect bounds = const Rect.fromLTWH(0, 0, 500, 350);

  @override
  String getTitle() => title;
  @override
  void setTitle(String value) => title = value;
  @override
  Rect getBounds() => bounds;
  @override
  Future<void> setPosition(Offset value) async {
    bounds = value & bounds.size;
  }

  @override
  Future<bool> setSize(Size value) async {
    bounds = bounds.topLeft & value;
    return true;
  }

  @override
  void focus() => focused = true;
  @override
  bool isFocused() => focused;
  @override
  Future<bool> closeWindow() async {
    closed = true;
    return true;
  }
}

void main() {
  test('all operations address the requested view, not main HWND', () async {
    final handles = <int, FakeWindowHandle>{
      1: FakeWindowHandle(1)..title = 'main',
      2: FakeWindowHandle(2)..title = 'child',
      3: FakeWindowHandle(3)..title = 'second child',
    };
    final service = ScopedNativeWindowController(resolve: (id) => handles[id]!);
    service.setTitle(2, 'detached editor');
    service.focus(3);
    await service.setPosition(2, const Offset(120, 330));
    expect(await service.setSize(2, const Size(640, 480)), isTrue);
    expect(await service.close(3), isTrue);
    expect(handles[1]!.title, 'main');
    expect(handles[2]!.title, 'detached editor');
    expect(handles[3]!.title, 'second child');
    expect(handles[2]!.bounds, const Rect.fromLTWH(120, 330, 640, 480));
    expect(handles[3]!.focused, isTrue);
    expect(handles[3]!.closed, isTrue);
    expect(handles[1]!.closed, isFalse);
  });

  test('reject invalid view IDs and resolve mismatches', () {
    final service = ScopedNativeWindowController(
      resolve: (_) => FakeWindowHandle(3),
    );
    expect(() => service.setTitle(0, 'invalid'), throwsArgumentError);
    expect(() => service.setTitle(-1, 'invalid'), throwsArgumentError);
    expect(() => service.setTitle(2, 'wrong'), throwsStateError);
  });
}
