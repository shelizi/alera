import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/text_actions/domain/text_actions_settings.dart';
import 'package:alera/src/features/text_actions/presentation/text_action_list_row.dart';
import 'package:alera/src/features/text_actions/presentation/text_actions_settings_pane.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('creates and saves a new text action', (tester) async {
    var settings = AleraSettings.defaults;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: StatefulBuilder(
            builder: (context, setState) => TextActionsSettingsPane(
              settings: settings.textActions,
              aiAssistSettings: settings.aiAssist,
              onChanged: (next) {
                setState(() {
                  settings = settings.copyWith(
                    textActions: next(settings.textActions),
                  );
                });
              },
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('New Action'));
    await tester.pumpAndSettle();
    expect(find.byType(AleraTextField), findsNWidgets(2));

    final nameField = find.byWidgetPredicate(
      (widget) => widget is TextField && widget.decoration?.labelText == 'Name',
    );
    final promptField = find.byWidgetPredicate(
      (widget) =>
          widget is TextField && widget.decoration?.labelText == 'Prompt',
    );
    await tester.enterText(nameField, 'Polish');
    await tester.enterText(promptField, 'Improve the selected text.');
    final saveButton = find.text('Save');
    await tester.ensureVisible(saveButton);
    await tester.tap(saveButton);
    await tester.pumpAndSettle();

    expect(settings.textActions.actions, hasLength(1));
    expect(settings.textActions.actions.single.name, 'Polish');
    expect(
      settings.textActions.actions.single.prompt,
      'Improve the selected text.',
    );
  });

  testWidgets('renders saved order and enabled state', (tester) async {
    const actions = TextActionsSettings(
      actions: <TextAction>[
        TextAction(id: 'one', name: 'First', prompt: 'First.'),
        TextAction(
          id: 'two',
          name: 'Second',
          prompt: 'Second.',
          enabled: false,
        ),
      ],
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: TextActionsSettingsPane(
            settings: actions,
            aiAssistSettings: AleraSettings.defaults.aiAssist,
            onChanged: (_) {},
          ),
        ),
      ),
    );

    expect(find.text('First'), findsOneWidget);
    expect(find.text('Second'), findsOneWidget);
    expect(find.text('Disabled'), findsOneWidget);
  });

  testWidgets(
    'renders Text Actions chrome and validation in Traditional Chinese',
    (tester) async {
      var settings = AleraSettings.defaults;
      await tester.pumpWidget(
        MaterialApp(
          locale: const Locale('zh', 'TW'),
          supportedLocales: supportedAleraLocales,
          localizationsDelegates: const <LocalizationsDelegate<dynamic>>[
            AleraLocalizationsDelegate(),
            ...GlobalMaterialLocalizations.delegates,
          ],
          home: Scaffold(
            body: StatefulBuilder(
              builder: (context, setState) => TextActionsSettingsPane(
                settings: settings.textActions,
                aiAssistSettings: settings.aiAssist,
                onChanged: (next) => setState(() {
                  settings = settings.copyWith(
                    textActions: next(settings.textActions),
                  );
                }),
              ),
            ),
          ),
        ),
      );

      expect(find.text('文字操作'), findsOneWidget);
      await tester.tap(find.text('新增動作'));
      await tester.pumpAndSettle();
      expect(find.text('動作'), findsOneWidget);
      expect(find.text('已啟用'), findsOneWidget);
      expect(find.textContaining('全域（'), findsWidgets);
      final saveButton = find.text('儲存');
      await tester.ensureVisible(saveButton);
      await tester.tap(saveButton);
      await tester.pump();
      expect(find.text('動作名稱為必填。'), findsOneWidget);
    },
  );

  testWidgets('duplicates, disables, and deletes a saved action', (
    tester,
  ) async {
    var actions = const TextActionsSettings(
      actions: <TextAction>[
        TextAction(id: 'one', name: 'Polish', prompt: 'Polish.'),
      ],
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: StatefulBuilder(
            builder: (context, setState) => TextActionsSettingsPane(
              settings: actions,
              aiAssistSettings: AleraSettings.defaults.aiAssist,
              onChanged: (next) => setState(() => actions = next(actions)),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.byTooltip('Duplicate'));
    await tester.pumpAndSettle();
    expect(actions.actions.map((action) => action.name), <String>[
      'Polish',
      'Polish Copy',
    ]);

    await tester.tap(
      find.descendant(
        of: find.byType(TextActionListRow).last,
        matching: find.byType(Switch),
      ),
    );
    await tester.pumpAndSettle();
    expect(actions.actions.last.enabled, isFalse);

    final deleteButton = find.widgetWithText(TextButton, 'Delete');
    await tester.ensureVisible(deleteButton);
    await tester.tap(deleteButton);
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, 'Delete'));
    await tester.pumpAndSettle();

    expect(actions.actions.single.id, 'one');
  });
}
