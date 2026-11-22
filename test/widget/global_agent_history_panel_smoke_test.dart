import 'package:alera/src/features/workbench/presentation/global_agent_history_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('History panel stays preview-only until explicit consent', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 960);
    tester.view.devicePixelRatio = 1;
    addTearDown(() {
      tester.view.resetPhysicalSize();
      tester.view.resetDevicePixelRatio();
    });

    await tester.pumpWidget(
      const ProviderScope(
        child: MaterialApp(home: Scaffold(body: GlobalAgentHistoryPanel())),
      ),
    );

    expect(find.text('Search History'), findsOneWidget);
    final search = find.widgetWithText(FilledButton, 'Search History');
    expect(tester.widget<FilledButton>(search).onPressed, isNull);
    expect(find.text('Find existing Tab'), findsNothing);
    expect(find.text('Open verified Tab'), findsNothing);

    await tester.tap(
      find.text('Allow local indexing of limited user-message previews'),
    );
    await tester.pump();
    expect(tester.widget<FilledButton>(search).onPressed, isNotNull);
    expect(find.text('Open verified Tab'), findsNothing);
  });
}
