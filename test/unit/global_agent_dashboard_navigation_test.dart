import 'package:alera/src/features/workbench/application/global_agent_dashboard_navigation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Dashboard navigation defaults to workbench and closes cleanly', () {
    final container = ProviderContainer();
    addTearDown(container.dispose);

    expect(container.read(globalAgentDashboardNavigationProvider), isFalse);

    final navigation = container.read(
      globalAgentDashboardNavigationProvider.notifier,
    );
    navigation.open();
    expect(container.read(globalAgentDashboardNavigationProvider), isTrue);

    navigation.open();
    expect(container.read(globalAgentDashboardNavigationProvider), isTrue);

    navigation.close();
    expect(container.read(globalAgentDashboardNavigationProvider), isFalse);
  });
}
