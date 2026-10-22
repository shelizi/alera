import 'package:alera_multiview_poc/main.dart' show sharedCounter;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

// This tests Dart state propagation only. Actual multi-window native lifecycle
// is exercised by the --poc-smoke integration harness in lib/main.dart.
void main() {
  testWidgets('one notifier broadcasts to multiple mounted views', (
    tester,
  ) async {
    sharedCounter.value = 0;
    addTearDown(() => sharedCounter.value = 0);
    await tester.pumpWidget(
      MaterialApp(
        home: Column(
          children: [
            ValueListenableBuilder<int>(
              valueListenable: sharedCounter,
              builder: (_, value, _) => Text('left:$value'),
            ),
            ValueListenableBuilder<int>(
              valueListenable: sharedCounter,
              builder: (_, value, _) => Text('right:$value'),
            ),
          ],
        ),
      ),
    );
    expect(find.text('left:0'), findsOneWidget);
    expect(find.text('right:0'), findsOneWidget);
    sharedCounter.value = 5;
    await tester.pump();
    expect(find.text('left:5'), findsOneWidget);
    expect(find.text('right:5'), findsOneWidget);
  });
}
