import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

const List<Locale> supportedAleraLocales = <Locale>[
  Locale('en'),
  Locale('zh', 'TW'),
];

Locale resolveAleraLocale(AppLanguage language, Locale? systemLocale) {
  return switch (language) {
    AppLanguage.english => const Locale('en'),
    AppLanguage.traditionalChinese => const Locale('zh', 'TW'),
    AppLanguage.system =>
      systemLocale?.languageCode.toLowerCase() == 'zh'
          ? const Locale('zh', 'TW')
          : const Locale('en'),
  };
}

class AleraLocalizations {
  const AleraLocalizations(this.locale);

  final Locale locale;

  bool get isTraditionalChinese => locale.languageCode.toLowerCase() == 'zh';

  String translate(String source) {
    if (!isTraditionalChinese) {
      return source;
    }
    return _traditionalChinese[source] ?? source;
  }

  static const Map<String, String> _traditionalChinese = <String, String>{
    'Settings': '設定',
    'Add Project': '新增專案',
    'Open in Zed': '在 Zed 中開啟',
    'Language': '語言',
    'App Language': '應用程式語言',
    'Follow System': '跟隨系統',
    'English': 'English',
    '繁體中文': '繁體中文',
    'Language used by the Alera interface.': 'Alera 介面使用的語言。',
    'Follow the system language or choose a language for Alera.':
        '跟隨系統語言，或為 Alera 指定語言。',
  };
}

class AleraLocalizationsDelegate
    extends LocalizationsDelegate<AleraLocalizations> {
  const AleraLocalizationsDelegate();

  @override
  bool isSupported(Locale locale) =>
      locale.languageCode == 'en' || locale.languageCode == 'zh';

  @override
  Future<AleraLocalizations> load(Locale locale) =>
      SynchronousFuture<AleraLocalizations>(AleraLocalizations(locale));

  @override
  bool shouldReload(AleraLocalizationsDelegate old) => false;
}

extension AleraLocalizationContext on BuildContext {
  AleraLocalizations get aleraLocalizations =>
      Localizations.of<AleraLocalizations>(this, AleraLocalizations) ??
      const AleraLocalizations(Locale('en'));

  String tr(String source) => aleraLocalizations.translate(source);
}
