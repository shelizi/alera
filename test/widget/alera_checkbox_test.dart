import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/design_system/forms/alera_checkbox.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

Widget _wrap(Widget child, {Locale? locale}) {
  return MaterialApp(
    locale: locale,
    supportedLocales: supportedAleraLocales,
    localizationsDelegates: const <LocalizationsDelegate<dynamic>>[
      AleraLocalizationsDelegate(),
      ...GlobalMaterialLocalizations.delegates,
    ],
    home: Scaffold(body: Center(child: child)),
  );
}

void main() {
  testWidgets('tapping toggles the value', (tester) async {
    bool? next;
    await tester.pumpWidget(
      _wrap(
        AleraCheckbox(
          value: false,
          label: 'Overwrite',
          onChanged: (value) => next = value,
        ),
      ),
    );

    await tester.tap(find.text('Overwrite'));
    expect(next, isTrue);
  });

  testWidgets('localizes its label at the render boundary', (tester) async {
    await tester.pumpWidget(
      _wrap(
        AleraCheckbox(value: false, label: 'Create Another', onChanged: (_) {}),
        locale: const Locale('zh', 'TW'),
      ),
    );

    expect(find.text('繼續建立下一個'), findsOneWidget);
    expect(find.text('Create Another'), findsNothing);
  });

  testWidgets('disabled checkbox ignores taps', (tester) async {
    bool? next;
    await tester.pumpWidget(
      _wrap(
        AleraCheckbox(
          value: true,
          label: 'Overwrite',
          enabled: false,
          onChanged: (value) => next = value,
        ),
      ),
    );

    await tester.tap(find.text('Overwrite'));
    expect(next, isNull);
  });
}
