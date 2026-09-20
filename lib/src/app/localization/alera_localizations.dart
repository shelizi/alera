import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

part 'alera_localizations_dynamic.dart';
part 'alera_localizations_dynamic_ja.dart';
part 'alera_localizations_dynamic_zh_cn.dart';
part 'alera_localizations_ja_agents.dart';
part 'alera_localizations_ja_settings.dart';
part 'alera_localizations_ja_settings_extra.dart';
part 'alera_localizations_ja_shell.dart';
part 'alera_localizations_zh_agents.dart';
part 'alera_localizations_zh_cn_agents.dart';
part 'alera_localizations_zh_cn_settings.dart';
part 'alera_localizations_zh_cn_settings_extra.dart';
part 'alera_localizations_zh_cn_shell.dart';
part 'alera_localizations_zh_settings.dart';
part 'alera_localizations_zh_settings_extra.dart';
part 'alera_localizations_zh_shell.dart';

const Locale _localeEnglish = Locale('en');
const Locale _localeTraditionalChinese = Locale('zh', 'TW');
const Locale _localeSimplifiedChinese = Locale('zh', 'CN');
const Locale _localeJapanese = Locale('ja', 'JP');

const List<Locale> supportedAleraLocales = <Locale>[
  _localeEnglish,
  _localeTraditionalChinese,
  _localeSimplifiedChinese,
  _localeJapanese,
];

/// Regions written with Simplified Chinese characters.
const Set<String> _simplifiedChineseRegions = <String>{'CN', 'SG', 'MY'};

/// Regions written with Traditional Chinese characters.
const Set<String> _traditionalChineseRegions = <String>{'TW', 'HK', 'MO'};

Locale resolveAleraLocale(AppLanguage language, Locale? systemLocale) {
  return switch (language) {
    AppLanguage.english => _localeEnglish,
    AppLanguage.traditionalChinese => _localeTraditionalChinese,
    AppLanguage.simplifiedChinese => _localeSimplifiedChinese,
    AppLanguage.japanese => _localeJapanese,
    AppLanguage.system => _resolveSystemAleraLocale(systemLocale),
  };
}

/// Maps a system locale onto the closest supported Alera locale.
///
/// Chinese resolves by script first (`Hans`/`Hant`), then by region, and a
/// bare `zh` keeps the historical Traditional Chinese default. Everything
/// outside the supported set falls back to English.
Locale _resolveSystemAleraLocale(Locale? systemLocale) {
  if (systemLocale == null) {
    return _localeEnglish;
  }
  final languageCode = systemLocale.languageCode.toLowerCase();
  if (languageCode == 'ja') {
    return _localeJapanese;
  }
  if (languageCode != 'zh') {
    return _localeEnglish;
  }
  final script = systemLocale.scriptCode?.toLowerCase();
  if (script == 'hans') {
    return _localeSimplifiedChinese;
  }
  if (script == 'hant') {
    return _localeTraditionalChinese;
  }
  final region = systemLocale.countryCode?.toUpperCase();
  if (region != null) {
    if (_simplifiedChineseRegions.contains(region)) {
      return _localeSimplifiedChinese;
    }
    if (_traditionalChineseRegions.contains(region)) {
      return _localeTraditionalChinese;
    }
  }
  return _localeTraditionalChinese;
}

/// The static/dynamic translation table backing an [AleraLocalizations].
enum _AleraTranslationTable {
  english,
  traditionalChinese,
  simplifiedChinese,
  japanese,
}

class AleraLocalizations {
  const AleraLocalizations(this.locale);

  final Locale locale;

  _AleraTranslationTable get _table {
    switch (locale.languageCode.toLowerCase()) {
      case 'ja':
        return _AleraTranslationTable.japanese;
      case 'zh':
        final script = locale.scriptCode?.toLowerCase();
        if (script == 'hans') {
          return _AleraTranslationTable.simplifiedChinese;
        }
        if (script == 'hant') {
          return _AleraTranslationTable.traditionalChinese;
        }
        final region = locale.countryCode?.toUpperCase();
        return region != null && _simplifiedChineseRegions.contains(region)
            ? _AleraTranslationTable.simplifiedChinese
            : _AleraTranslationTable.traditionalChinese;
      default:
        return _AleraTranslationTable.english;
    }
  }

  bool get isTraditionalChinese =>
      _table == _AleraTranslationTable.traditionalChinese;

  bool get isSimplifiedChinese =>
      _table == _AleraTranslationTable.simplifiedChinese;

  bool get isJapanese => _table == _AleraTranslationTable.japanese;

  String translate(String source) {
    return switch (_table) {
      _AleraTranslationTable.english => source,
      _AleraTranslationTable.traditionalChinese =>
        _traditionalChinese[source] ??
            _traditionalChineseSettingsExtra[source] ??
            _translateDynamicTraditionalChinese(source) ??
            source,
      _AleraTranslationTable.simplifiedChinese =>
        _simplifiedChinese[source] ??
            _simplifiedChineseSettingsExtra[source] ??
            _translateDynamicSimplifiedChinese(source) ??
            source,
      _AleraTranslationTable.japanese =>
        _japanese[source] ??
            _japaneseSettingsExtra[source] ??
            _translateDynamicJapanese(source) ??
            source,
    };
  }

  /// Static table lookup used by the dynamic pattern rules to translate the
  /// embedded label of a composed string. Falls back to the English source so
  /// runtime values such as branch names pass through untouched.
  static String _lookupTraditionalChinese(String source) =>
      _traditionalChinese[source] ?? source;

  static String _lookupSimplifiedChinese(String source) =>
      _simplifiedChinese[source] ?? source;

  static String _lookupJapanese(String source) => _japanese[source] ?? source;

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

  static const Map<String, String> _simplifiedChinese = <String, String>{
    // Common actions and navigation.
    'Settings': '设置',
    'Close': '关闭',
    'Cancel': '取消',
    'Save': '保存',
    'Delete': '删除',
    'Remove': '移除',
    'Rename': '重命名',
    'Refresh': '刷新',
    'Retry': '重试',
    'Try Again': '再试一次',
    'Copy': '复制',
    'Cut': '剪切',
    'Paste': '粘贴',
    'Browse': '浏览',
    'Search': '搜索',
    'Clear': '清除',
    'Create': '创建',
    'Continue': '继续',
    'Open': '打开',
    'Export': '导出',
    'Import': '导入',
    'Apply': '应用',
    'Reset': '重置',
    'Reset to Default': '重置为默认值',
    'Preferences': '偏好设置',
    'Resources': '资源',
    'Search settings': '搜索设置',
    'No matching settings.': '没有匹配的设置。',
    'No settings found.': '未找到设置。',

    ..._simplifiedChineseShell,
    ..._simplifiedChineseAgents,
    ..._simplifiedChineseSettings,
  };

  static const Map<String, String> _japanese = <String, String>{
    // Common actions and navigation.
    'Settings': '設定',
    'Close': '閉じる',
    'Cancel': 'キャンセル',
    'Save': '保存',
    'Delete': '削除',
    'Remove': '取り外す',
    'Rename': '名前を変更',
    'Refresh': '再読み込み',
    'Retry': '再試行',
    'Try Again': 'もう一度試す',
    'Copy': 'コピー',
    'Cut': '切り取り',
    'Paste': '貼り付け',
    'Browse': '参照',
    'Search': '検索',
    'Clear': 'クリア',
    'Create': '作成',
    'Continue': '続行',
    'Open': '開く',
    'Export': 'エクスポート',
    'Import': 'インポート',
    'Apply': '適用',
    'Reset': 'リセット',
    'Reset to Default': '既定値にリセット',
    'Preferences': '環境設定',
    'Resources': 'リソース',
    'Search settings': '設定を検索',
    'No matching settings.': '一致する設定がありません。',
    'No settings found.': '設定が見つかりません。',

    ..._japaneseShell,
    ..._japaneseAgents,
    ..._japaneseSettings,
  };

  /// Source keys covered by every static table, used by parity tests.
  @visibleForTesting
  static Set<String> staticSourceKeysFor(AppLanguage language) {
    switch (language) {
      case AppLanguage.traditionalChinese:
        return <String>{
          ..._traditionalChinese.keys,
          ..._traditionalChineseSettingsExtra.keys,
        };
      case AppLanguage.simplifiedChinese:
        return <String>{
          ..._simplifiedChinese.keys,
          ..._simplifiedChineseSettingsExtra.keys,
        };
      case AppLanguage.japanese:
        return <String>{..._japanese.keys, ..._japaneseSettingsExtra.keys};
      case AppLanguage.english:
      case AppLanguage.system:
        return const <String>{};
    }
  }
}

class AleraLocalizationsDelegate
    extends LocalizationsDelegate<AleraLocalizations> {
  const AleraLocalizationsDelegate();

  @override
  bool isSupported(Locale locale) =>
      locale.languageCode == 'en' ||
      locale.languageCode == 'zh' ||
      locale.languageCode == 'ja';

  @override
  Future<AleraLocalizations> load(Locale locale) =>
      SynchronousFuture<AleraLocalizations>(AleraLocalizations(locale));

  @override
  bool shouldReload(AleraLocalizationsDelegate old) => false;
}

extension AleraLocalizationContext on BuildContext {
  AleraLocalizations get aleraLocalizations =>
      Localizations.of<AleraLocalizations>(this, AleraLocalizations) ??
      const AleraLocalizations(_localeEnglish);

  String tr(String source) => aleraLocalizations.translate(source);
}
