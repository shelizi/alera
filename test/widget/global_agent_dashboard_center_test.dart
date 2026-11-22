import 'package:alera/src/features/workbench/application/global_agent_dashboard_navigation.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_dashboard_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('Global Agent Dashboard is a center page, not a modal dialog', (
    tester,
  ) async {
    final container = ProviderContainer();
    addTearDown(container.dispose);
    container.read(globalAgentDashboardNavigationProvider.notifier).open();

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(
          home: Scaffold(body: GlobalAgentDashboardPage()),
        ),
      ),
    );

    expect(find.text('Global Agent Dashboard'), findsOneWidget);
    expect(find.byType(Dialog), findsNothing);
    expect(find.byTooltip('Close'), findsOneWidget);

    await tester.tap(find.byTooltip('Close'));
    await tester.pump();
    expect(container.read(globalAgentDashboardNavigationProvider), isFalse);
  });
}
