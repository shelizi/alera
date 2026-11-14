import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/settings/presentation/panes/agy_quota_profiles_control.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('Add Google Account creates pending metadata only', (
    tester,
  ) async {
    var registrations = <AgyQuotaProfileSettings>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: AgyQuotaProfilesControl(
            profiles: const [],
            selectedAccountId: 'default',
            onSelected: (_) {},
            onChanged: (updated) => registrations = updated,
          ),
        ),
      ),
    );
    expect(
      find.text('No additional Antigravity accounts registered'),
      findsOneWidget,
    );
    await tester.tap(find.text('Add Google Account'));
    await tester.pumpAndSettle();
    expect(
      find.textContaining('This does not sign in to Google'),
      findsOneWidget,
    );

    final fields = find.byType(TextField);
    await tester.enterText(fields.at(0), 'Research');
    await tester.enterText(fields.at(1), '../invalid');
    await tester.tap(find.text('Save Registration'));
    await tester.pumpAndSettle();
    expect(registrations, isEmpty);
    expect(find.textContaining('unique account ID'), findsOneWidget);

    await tester.enterText(fields.at(1), 'research');
    await tester.tap(find.text('Save Registration'));
    await tester.pumpAndSettle();
    expect(registrations, hasLength(1));
    expect(registrations.single.alias, 'Research');
    expect(registrations.single.accountId, 'research');
  });
}
