import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const labels = <String>[
    'Copy Selected Files/Folders',
    'Copy File/Folder',
    'Cut Selected Files/Folders',
    'Cut File/Folder',
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
