import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const menuKeys = <String>[
    'Split Up',
    'Split Down',
    'Split Left',
    'Split Right',
    'Move to Main Window',
    'Move to New Window',
    'Open In External Terminal',
    'Keep Open',
    'Pin Tab',
    'Unpin Tab',
    'Reload Document',
    'Open with Default Application',
    'Reveal in {fileManager}',
    'View File History',
    'Compare With Latest Git Revision',
    'Compare With Branch...',
    'Close',
    'Close Others',
    'Close Tabs to the Right',
    'Change Title',
    'Generate Title',
    'Regenerate Title',
    'Generating title...',
    'Change Terminal Title',
    'Terminal Title',
    'Could not move the tab back to the main window.',
    'Could not generate title: {error}',
    'Could not open detached window.',
    'Could not open detached window: {error}',
    'Save this file before moving it to a new window.',
    'Could not open item with the default application.',
    'Could not reveal item in file manager.',
  ];

  for (final locale in <Locale>[
    const Locale('zh', 'TW'),
    const Locale('zh', 'CN'),
    const Locale('ja', 'JP'),
  ]) {
    test('Classic Tab menu has no missing translations for $locale', () {
      final translations = AleraLocalizations(locale);
      for (final key in menuKeys) {
        expect(
          translations.translate(key),
          isNot(key),
          reason: '$locale is missing the Tab menu key: $key',
        );
      }
    });

    test('Classic Tab menu interpolates runtime labels for $locale', () {
      final translations = AleraLocalizations(locale);
      final label = translations.translateTemplate('Reveal in {fileManager}', {
        'fileManager': 'Explorer',
      });
      expect(label, contains('Explorer'));
      expect(label, isNot(contains('{fileManager}')));

      final error = translations.translateTemplate(
        'Could not generate title: {error}',
        {'error': 'E_TIMEOUT'},
      );
      expect(error, contains('E_TIMEOUT'));
      expect(error, isNot(contains('{error}')));
    });
  }

  test('English Tab menu retains its source labels', () {
    const translations = AleraLocalizations(Locale('en'));
    for (final key in menuKeys) {
      expect(translations.translate(key), key);
    }
  });
}
