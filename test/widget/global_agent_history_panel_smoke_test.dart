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
    expect(
      find.byKey(const ValueKey('history-advanced-filters')),
      findsNothing,
    );

    await tester.tap(find.text('Advanced filters'));
    await tester.pump();
    expect(
      find.byKey(const ValueKey('history-advanced-filters')),
      findsOneWidget,
    );
    expect(find.text('Project ID'), findsOneWidget);
    expect(find.text('Worktree ID'), findsOneWidget);

    await tester.tap(find.text('Advanced filters'));
    await tester.pump();
    expect(
      find.byKey(const ValueKey('history-advanced-filters')),
      findsNothing,
    );

    await tester.tap(
      find.text('Allow local indexing of limited user-message previews'),
    );
    await tester.pump();
    expect(tester.widget<FilledButton>(search).onPressed, isNotNull);
    expect(find.text('Open verified Tab'), findsNothing);

    // Clearing local index is a deliberate confirmed action, not one tap.
    await tester.tap(find.text('Clear local index'));
    await tester.pumpAndSettle();
    expect(find.text('Clear local history index?'), findsOneWidget);
    expect(
      find.text('Claude and Codex source conversations will not be changed.'),
      findsNothing,
    );
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(find.text('Clear local history index?'), findsNothing);
    expect(tester.widget<FilledButton>(search).onPressed, isNotNull);
  });
}
