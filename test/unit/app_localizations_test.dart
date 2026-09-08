import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('Alera localization', () {
    test('resolves configured language and system fallback', () {
      expect(resolveAleraLocale(AppLanguage.english, const Locale('zh', 'TW')), const Locale('en'));
      expect(resolveAleraLocale(AppLanguage.traditionalChinese, const Locale('en', 'US')), const Locale('zh', 'TW'));
      expect(resolveAleraLocale(AppLanguage.system, const Locale('zh', 'TW')), const Locale('zh', 'TW'));
      expect(resolveAleraLocale(AppLanguage.system, const Locale('ja', 'JP')), const Locale('en'));
    });

    test('traditional Chinese translates known UI strings', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Settings'), '設定');
      expect(l10n.translate('Add Project'), '新增專案');
      expect(l10n.translate('Open in Zed'), '在 Zed 中開啟');
    });

    test('English and unknown strings fall back to source text', () {
      final en = AleraLocalizations(const Locale('en'));
      final zh = AleraLocalizations(const Locale('zh', 'TW'));
      expect(en.translate('Settings'), 'Settings');
      expect(zh.translate('Unmapped String'), 'Unmapped String');
    });
  });
}
