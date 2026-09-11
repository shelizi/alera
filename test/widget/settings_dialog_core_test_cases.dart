part of 'settings_dialog_test.dart';

void _registerSettingsDialogCoreTests() {
  testWidgets('renders core settings navigation in Traditional Chinese', (
    tester,
  ) async {
    await _pumpSettingsDialog(tester, locale: const Locale('zh', 'TW'));

    expect(find.text('設定'), findsOneWidget);
    expect(find.text('應用程式'), findsWidgets);
    expect(find.text('語言'), findsWidgets);
    expect(find.text('應用程式語言'), findsOneWidget);
    expect(find.text('跟隨系統'), findsOneWidget);
    expect(find.text('偏好設定'), findsOneWidget);

    await tester.enterText(find.byType(TextField).first, '語言');
    await tester.pump();

    expect(find.text('應用程式'), findsWidgets);
    expect(find.text('應用程式語言'), findsOneWidget);
    expect(find.text('終端機'), findsNothing);
  });

  testWidgets('edits destructive confirmation settings', (tester) async {
    final container = await _pumpSettingsDialog(tester);

    // 'Safety' appears both as a subsection chip and as the group title.
    expect(find.text('Safety'), findsWidgets);
    expect(find.text('Confirm Project Removal'), findsOneWidget);
    expect(find.text('Confirm Workspace Removal'), findsOneWidget);

    await tester.ensureVisible(find.byType(Switch).at(0));
    await tester.pump();
    await tester.tap(find.byType(Switch).at(0));
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).general.confirmProjectRemoval,
      isFalse,
    );

    await tester.ensureVisible(find.byType(Switch).at(1));
    await tester.pump();
    final workspaceConfirmation = tester.widget<Switch>(
      find.byType(Switch).at(1),
    );
    expect(workspaceConfirmation.value, isTrue);
    expect(workspaceConfirmation.onChanged, isNull);

    await tester.enterText(find.byType(TextField).first, 'destructive');
    await tester.pump();

    expect(find.text('Confirm Project Removal'), findsOneWidget);
    expect(find.text('Confirm Workspace Removal'), findsOneWidget);
  });

  testWidgets('workspace directory commits and clears overrides', (
    tester,
  ) async {
    final container = await _pumpSettingsDialog(tester);
    final field = find.byType(TextField).at(1);

    await tester.enterText(field, '/tmp/alera-workspaces');
    await tester.testTextInput.receiveAction(.done);
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).general.workspaceDirectory,
      '/tmp/alera-workspaces',
    );

    await tester.enterText(field, '   ');
    await tester.testTextInput.receiveAction(.done);
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).general.workspaceDirectory,
      isNull,
    );
  });

  testWidgets(
    'support section can star Alera from not-starred and error states',
    (tester) async {
      final container = await _pumpSettingsDialog(
        tester,
        starController: _FakeGitHubStarController(
          .notStarred,
          nextStarState: .starred,
        ),
      );

      expect(find.text('Support Alera'), findsOneWidget);
      expect(find.text('Star'), findsOneWidget);

      await tester.ensureVisible(find.text('Star'));
      await tester.pump();
      await tester.tap(find.text('Star'));
      await tester.pump(const Duration(milliseconds: 100));

      expect(find.text('Thanks for the support!'), findsOneWidget);
      expect(
        container.read(settingsControllerProvider).general.starClicked,
        isTrue,
      );

      final errorController = _FakeGitHubStarController(
        .error,
        nextStarState: .starred,
      );
      container.dispose();
      await _pumpSettingsDialog(tester, starController: errorController);

      expect(find.text('Try again'), findsOneWidget);

      await tester.ensureVisible(find.text('Try again'));
      await tester.pump();
      await tester.tap(find.text('Try again'));
      await tester.pump(const Duration(milliseconds: 100));

      expect(find.text('Thanks for the support!'), findsOneWidget);
    },
  );

  testWidgets('support section hides itself when starring is unavailable', (
    tester,
  ) async {
    await _pumpSettingsDialog(
      tester,
      starController: _FakeGitHubStarController(.hidden),
    );

    expect(find.text('Support Alera'), findsNothing);
    expect(find.byKey(const ValueKey<String>('hidden')), findsNothing);
  });

  testWidgets('sidebar groups sections under Preferences and Resources', (
    tester,
  ) async {
    await _pumpSettingsDialog(tester);

    // AleraSectionHeader renders group labels uppercased.
    expect(find.text('PREFERENCES'), findsOneWidget);
    expect(find.text('RESOURCES'), findsOneWidget);
    expect(find.text('Application'), findsWidgets);
    expect(find.text('Agents'), findsOneWidget);
    expect(find.text('Mobile Devices'), findsOneWidget);
    expect(find.text('General'), findsNothing);

    // Let pending timers (tooltips, animations) finish before teardown.
    await tester.pump(const Duration(seconds: 2));
  });

  testWidgets('searching qr surfaces the mobile devices section', (
    tester,
  ) async {
    await _pumpSettingsDialog(tester);

    await tester.enterText(find.byType(TextField).first, 'qr');
    await tester.pump();

    expect(find.text('Mobile Devices'), findsWidgets);
    expect(find.text('Remote Hosts'), findsNothing);
  });

  testWidgets('tapping a subsection chip scrolls its group into view', (
    tester,
  ) async {
    await _pumpSettingsDialog(tester);
    await _selectTerminalSectionByNav(tester);

    // 'Advanced' appears as a header chip and as the (initially offscreen)
    // group title further down the pane.
    final advancedTexts = find.text('Advanced', skipOffstage: false);
    expect(advancedTexts, findsNWidgets(2));

    final chip = advancedTexts.first;
    final groupTitle = advancedTexts.last;
    final beforeDy = tester.getTopLeft(groupTitle).dy;

    await tester.tap(chip);
    await tester.pumpAndSettle();

    final afterDy = tester.getTopLeft(groupTitle).dy;
    expect(afterDy, lessThan(beforeDy));
    final screenHeight =
        tester.view.physicalSize.height / tester.view.devicePixelRatio;
    expect(afterDy, lessThan(screenHeight));
  });

  testWidgets('hidden star control shrinks away', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: buildStarControlForTesting(state: .hidden, onStar: () {}),
        ),
      ),
    );

    expect(find.byKey(const ValueKey<String>('hidden')), findsOneWidget);
  });
}
