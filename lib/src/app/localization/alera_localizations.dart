import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

part 'alera_localizations_dynamic.dart';
part 'alera_localizations_zh_agents.dart';
part 'alera_localizations_zh_settings.dart';
part 'alera_localizations_zh_settings_extra.dart';
part 'alera_localizations_zh_shell.dart';

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
    return _traditionalChinese[source] ??
        _traditionalChineseSettingsExtra[source] ??
        _translateDynamicTraditionalChinese(source) ??
        source;
  }

  static const Map<String, String> _traditionalChinese = <String, String>{
    // Common actions and navigation.
    'Settings': '設定',
    'Close': '關閉',
    'Cancel': '取消',
    'Save': '儲存',
    'Delete': '刪除',
    'Remove': '移除',
    'Rename': '重新命名',
    'Refresh': '重新整理',
    'Retry': '重試',
    'Try Again': '再試一次',
    'Copy': '複製',
    'Cut': '剪下',
    'Paste': '貼上',
    'Browse': '瀏覽',
    'Search': '搜尋',
    'Clear': '清除',
    'Create': '建立',
    'Continue': '繼續',
    'Open': '開啟',
    'Export': '匯出',
    'Import': '匯入',
    'Apply': '套用',
    'Reset': '重設',
    'Reset to Default': '重設為預設值',
    'Preferences': '偏好設定',
    'Resources': '資源',
    'Search settings': '搜尋設定',
    'No matching settings.': '沒有符合的設定。',
    'No settings found.': '找不到設定。',

    ..._traditionalChineseShell,
    ..._traditionalChineseAgents,
    ..._traditionalChineseSettings,
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
