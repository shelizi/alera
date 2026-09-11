part of 'settings_dialog_test.dart';

void _registerSettingsDialogTerminalTests() {
  testWidgets('edits additional terminal numeric and color overrides', (
    tester,
  ) async {
    final container = await _pumpSettingsDialog(tester);
    await _selectTerminalSection(tester);
    final before = container.read(settingsControllerProvider).terminal;

    // Scope to the i-th number field: the up/down chevrons are shared with
    // expander rows, so a global icon index would no longer be unique.
    Future<void> tapStepper(IconData icon, int index) async {
      final finder = find.descendant(
        of: find.byType(AleraNumberField).at(index),
        matching: find.byIcon(icon),
      );
      await tester.ensureVisible(finder);
      await tester.pumpAndSettle();
      await tester.tap(finder);
      await tester.pump(const Duration(milliseconds: 50));
    }

    await tapStepper(AleraIcons.chevronUp, 1);
    await tapStepper(AleraIcons.chevronUp, 2);
    await tapStepper(AleraIcons.chevronDown, 3);
    await tapStepper(AleraIcons.chevronDown, 4);
    await tapStepper(AleraIcons.chevronUp, 5);
    await tapStepper(AleraIcons.chevronUp, 6);
    await tapStepper(AleraIcons.chevronUp, 7);
    await tapStepper(AleraIcons.chevronUp, 8);
    await tapStepper(AleraIcons.chevronUp, 9);

    Future<void> setSwatchColor(int index, Color color) async {
      final swatch = find.byType(AleraColorSwatch).at(index);
      await tester.ensureVisible(swatch);
      await tester.pumpAndSettle();
      await tester.tap(swatch);
      await tester.pumpAndSettle();
      final picker = tester.widget<ColorPicker>(find.byType(ColorPicker));
      picker.onColorChanged(color);
      await tester.pump();
      await tester.tap(find.text('Select'));
      await tester.pumpAndSettle();
    }

    await setSwatchColor(1, const Color(0xFF223344));
    await setSwatchColor(2, const Color(0xFF445566));
    await setSwatchColor(3, const Color(0xFF667788));

    final after = container.read(settingsControllerProvider).terminal;
    expect(after.fontWeight, greaterThan(before.fontWeight));
    expect(after.lineHeight, greaterThan(before.lineHeight));
    expect(after.cursorOpacity, lessThan(before.cursorOpacity));
    expect(after.backgroundOpacity, lessThan(before.backgroundOpacity));
    expect(after.paddingX, greaterThan(before.paddingX));
    expect(after.paddingY, greaterThan(before.paddingY));
    expect(
      after.tuiScrollSensitivity,
      greaterThan(before.tuiScrollSensitivity),
    );
    expect(after.scrollbackLines, greaterThan(before.scrollbackLines));
    expect(after.hostScrollbackBytes, greaterThan(before.hostScrollbackBytes));
    expect(after.colorOverrides.background, '#223344');
    expect(after.colorOverrides.cursor, '#445566');
    expect(after.colorOverrides.selection, '#667788');
  });

  testWidgets(
    'edits runtime host lifecycle settings from the application pane',
    (tester) async {
      final container = await _pumpSettingsDialog(tester);
      final before = container.read(settingsControllerProvider).terminal;
      expect(before.keepRuntimeOpenOnAppQuit, isFalse);

      await tester.ensureVisible(find.text('Keep Computer Awake'));
      await tester.pumpAndSettle();
      expect(find.text('Keep Computer Awake'), findsOneWidget);

      final toggle = find.descendant(
        of: find.ancestor(
          of: find.text('Keep Runtime Open When App Quits'),
          matching: find.byType(SettingsSwitchRow),
        ),
        matching: find.byType(Switch),
      );
      await tester.ensureVisible(toggle);
      await tester.pumpAndSettle();
      await tester.tap(toggle);
      await tester.pumpAndSettle();

      Future<void> bumpDelay(String title) async {
        final stepper = find.descendant(
          of: find.ancestor(
            of: find.text(title),
            matching: find.byType(SettingsIntegerRow),
          ),
          matching: find.byIcon(AleraIcons.chevronUp),
        );
        await tester.ensureVisible(stepper);
        await tester.pumpAndSettle();
        await tester.tap(stepper);
        await tester.pump(const Duration(milliseconds: 50));
      }

      await bumpDelay('Empty Host Shutdown');
      await bumpDelay('Detached Session Shutdown');

      final after = container.read(settingsControllerProvider).terminal;
      expect(after.keepRuntimeOpenOnAppQuit, isTrue);
      expect(
        after.hostEmptyShutdownDelaySeconds,
        greaterThan(before.hostEmptyShutdownDelaySeconds),
      );
      expect(
        after.hostDetachedSessionShutdownDelaySeconds,
        greaterThan(before.hostDetachedSessionShutdownDelaySeconds),
      );
    },
  );

  testWidgets('shows terminal settings and filters with search', (
    tester,
  ) async {
    await _pumpSettingsDialog(tester);

    // 'Updates' appears both as a subsection chip and as the group title.
    expect(find.text('Updates'), findsWidgets);

    await _selectTerminalSectionByNav(tester);

    expect(find.text('Terminal'), findsWidgets);
    expect(find.text('Font Family'), findsOneWidget);
    expect(find.text('Theme Preset'), findsOneWidget);
    expect(find.text('TUI Scroll Speed'), findsOneWidget);
    expect(find.text('Copy On Select'), findsOneWidget);
    expect(find.text('Allow OSC 52 Clipboard Writes'), findsOneWidget);
    expect(find.text('Scrollback Lines'), findsOneWidget);

    await tester.enterText(find.byType(TextField).first, 'cursor');
    await tester.pump();

    expect(find.text('Terminal'), findsWidgets);
    expect(find.text('Cursor Shape'), findsOneWidget);
    expect(find.text('Cursor Opacity'), findsOneWidget);

    await tester.enterText(find.byType(TextField).first, 'missing setting');
    await tester.pump();

    expect(find.text('No settings found.'), findsOneWidget);
  });

  testWidgets('edits and resets terminal settings', (tester) async {
    final container = await _pumpSettingsDialog(tester);
    await _selectTerminalSectionByNav(tester);

    await tester.enterText(find.byType(TextField).at(2), '18');
    await tester.testTextInput.receiveAction(.done);
    await tester.pump(const Duration(milliseconds: 50));

    expect(container.read(settingsControllerProvider).terminal.fontSize, 18);

    await tester.tap(
      find.byKey(const ValueKey<String>('terminal-font-family-field')),
    );
    await tester.pump();
    await tester.enterText(
      find.byKey(const ValueKey<String>('terminal-font-family-field')),
      'Men',
    );
    await tester.pump();
    await tester.tap(find.text('Menlo'));
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).terminal.fontFamily,
      'Menlo',
    );

    await tester.ensureVisible(find.byTooltip('Bar'));
    await tester.pump();
    await tester.tap(find.byTooltip('Bar'));
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).terminal.cursorShape,
      TerminalCursorShape.bar,
    );

    await tester.ensureVisible(find.text('Blinking Cursor'));
    await tester.pump();
    await tester.tap(find.byType(Switch).first);
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).terminal.cursorBlink,
      isTrue,
    );

    await tester.ensureVisible(find.text('Theme Preset'));
    await tester.pump();
    await tester.enterText(
      find.byKey(const ValueKey<String>('terminal-theme-search-field')),
      'dracula',
    );
    await tester.pump();
    await tester.tap(find.text('Dracula'));
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).terminal.themeName,
      TerminalThemeNames.dracula,
    );

    await tester.dragFrom(const Offset(500, 240), const Offset(0, 1000));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Reset Terminal'));
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).terminal.fontSize,
      TerminalSettings.defaults.fontSize,
    );
    expect(
      container.read(settingsControllerProvider).terminal.cursorShape,
      TerminalCursorShape.block,
    );
    expect(
      container.read(settingsControllerProvider).terminal.themeName,
      TerminalSettings.defaults.themeName,
    );
  });

  testWidgets('theme picker stacks preview below the list on narrow widths', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildAleraDarkTheme(),
        home: Scaffold(
          body: SizedBox(
            width: 540,
            child: buildThemePickerSettingForTesting(
              value: TerminalThemeNames.aleraDark,
              onChanged: (_) {},
            ),
          ),
        ),
      ),
    );
    await tester.pump();

    expect(
      find.byKey(const ValueKey<String>('terminal-theme-search-field')),
      findsOneWidget,
    );
    expect(find.text('Theme Preset'), findsOneWidget);
  });

  testWidgets('theme settings localize chrome and preserve theme names', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        locale: const Locale('zh', 'TW'),
        supportedLocales: supportedAleraLocales,
        localizationsDelegates: const <LocalizationsDelegate<dynamic>>[
          AleraLocalizationsDelegate(),
          ...GlobalMaterialLocalizations.delegates,
        ],
        theme: buildAleraDarkTheme(),
        home: Scaffold(
          body: SizedBox(
            width: 540,
            child: buildThemePickerSettingForTesting(
              value: TerminalThemeNames.aleraDark,
              onChanged: (_) {},
            ),
          ),
        ),
      ),
    );
    await tester.pump();

    expect(find.text('主題預設'), findsOneWidget);
    expect(find.text('搜尋並選擇內建的終端機配色主題。'), findsOneWidget);
    expect(find.text('已選取：${TerminalThemeNames.aleraDark}'), findsOneWidget);
    expect(find.textContaining('顯示 '), findsOneWidget);
    expect(find.text(TerminalThemeNames.aleraDark), findsWidgets);
  });

  testWidgets('terminal theme controls localize labels and tooltips', (
    tester,
  ) async {
    await _pumpSettingsDialog(tester, locale: const Locale('zh', 'TW'));
    await _selectTerminalSectionByNav(tester);

    await tester.enterText(find.byType(TextField).first, 'cursor');
    await tester.pump();
    expect(find.text('游標形狀'), findsOneWidget);
    await tester.ensureVisible(find.byTooltip('直線'));
    await tester.pump();
    expect(find.byTooltip('直線'), findsOneWidget);
  });

  testWidgets('edits terminal color override via color picker dialog', (
    tester,
  ) async {
    final container = await _pumpSettingsDialog(tester);
    await _selectTerminalSectionByNav(tester);

    // Verify initial state: no foreground color override
    expect(
      container
          .read(settingsControllerProvider)
          .terminal
          .colorOverrides
          .foreground,
      isNull,
    );

    // Find the first color swatch (Foreground color swatch)
    final swatchFinder = find.byType(AleraColorSwatch).first;
    expect(swatchFinder, findsOneWidget);

    // Ensure the color swatch is visible
    await tester.ensureVisible(swatchFinder);
    await tester.pumpAndSettle();

    // Tap it to open the color picker dialog
    await tester.tap(swatchFinder);
    await tester.pumpAndSettle();

    // Verify the dialog has opened
    expect(find.text('Foreground Color'), findsWidgets); // dialog title
    expect(find.byType(ColorPicker), findsOneWidget);

    // Simulate changing color in the picker to #112233
    final ColorPicker pickerWidget = tester.widget(find.byType(ColorPicker));
    pickerWidget.onColorChanged(const Color(0xFF112233));
    await tester.pump();

    // Tap Select
    await tester.tap(find.text('Select'));
    await tester.pumpAndSettle();

    // Verify color override is updated to #112233
    expect(
      container
          .read(settingsControllerProvider)
          .terminal
          .colorOverrides
          .foreground,
      '#112233',
    );
  });

  testWidgets(
    'font autocomplete supports keyboard selection and empty-state dismissal',
    (tester) async {
      final container = await _pumpSettingsDialog(tester);
      await _selectTerminalSectionByNav(tester);

      final field = find.byKey(
        const ValueKey<String>('terminal-font-family-field'),
      );
      await tester.tap(field);
      await tester.pump();
      await tester.enterText(field, 'sf');
      await tester.pump();

      expect(find.text('SF Mono'), findsWidgets);

      await tester.sendKeyDownEvent(.arrowDown);
      await tester.sendKeyUpEvent(.arrowDown);
      await tester.sendKeyDownEvent(.enter);
      await tester.sendKeyUpEvent(.enter);
      await tester.pump(const Duration(milliseconds: 50));

      expect(
        container.read(settingsControllerProvider).terminal.fontFamily,
        'SF Mono',
      );

      await tester.tap(field);
      await tester.pump();
      await tester.enterText(field, 'zzz');
      await tester.pump();

      expect(find.text('No matching fonts.'), findsOneWidget);

      await tester.sendKeyDownEvent(.escape);
      await tester.sendKeyUpEvent(.escape);
      await tester.pump();

      expect(find.text('No matching fonts.'), findsNothing);
    },
  );
}
