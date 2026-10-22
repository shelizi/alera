import 'package:alera/src/features/workbench/application/detached_tab_drag_preview.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('dev.leynier.alera/detached_tabs');

  test(
    'native ghost receives the dragged tab title and stops on release',
    () async {
      final calls = <MethodCall>[];
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, (call) async {
            calls.add(call);
            return null;
          });
      addTearDown(
        () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
            .setMockMethodCallHandler(channel, null),
      );

      const preview = DetachedTabDragPreview(enabled: true);
      await preview.start('My readme.md');
      await preview.stop();

      expect(calls.map((call) => call.method), [
        'startDragPreview',
        'stopDragPreview',
      ]);
      expect(calls.first.arguments, {'title': 'My readme.md'});
    },
  );

  test('preview remains optional when native channel is unavailable', () async {
    const preview = DetachedTabDragPreview(enabled: true);
    await expectLater(preview.start('readme.md'), completes);
    await expectLater(preview.stop(), completes);
  });

  test('no native preview calls on unsupported platforms', () async {
    const preview = DetachedTabDragPreview(enabled: false);
    await expectLater(preview.start('readme.md'), completes);
    await expectLater(preview.stop(), completes);
  });
}
