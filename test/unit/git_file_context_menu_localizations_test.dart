import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const labels = <String>[
    'Copy',
    'Cut',
    'Paste into Containing Folder / Selected Folder',
    'Copy Relative Path',
    'Add to .gitignore',
    'Unstage Selected',
    'Stage Selected',
    'Discard Selected',
    'Open File',
    'Open with Default Application',
    'Reveal in Explorer',
    'View File History',
    'Compare With Latest Git Revision',
    'Compare With Branch...',
    'Unstage',
    'Stage',
    'Discard',
  ];

  test('Copy and Cut use short labels in all supported locales', () {
    for (final (locale, copy, cut) in <(Locale, String, String)>[
      (const Locale('en'), 'Copy', 'Cut'),
      (const Locale('zh', 'TW'), '複製', '剪下'),
      (const Locale('zh', 'CN'), '复制', '剪切'),
      (const Locale('ja', 'JP'), 'コピー', '切り取り'),
    ]) {
      final strings = AleraLocalizations(locale);
      expect(strings.translate('Copy'), copy);
      expect(strings.translate('Cut'), cut);
    }
  });

  test('Git file and folder context menu labels follow all four locales', () {
    for (final label in labels) {
      expect(const AleraLocalizations(Locale('en')).translate(label), label);
      for (final locale in <Locale>[
        const Locale('zh', 'TW'),
        const Locale('zh', 'CN'),
        const Locale('ja', 'JP'),
      ]) {
        final localized = AleraLocalizations(locale).translate(label);
        expect(localized, isNot(label), reason: '$locale: $label');
        expect(localized.trim(), isNotEmpty);
      }
    }
  });
}
