import 'package:dart_mappable/dart_mappable.dart';

import 'language_id.dart';

part 'language_intelligence_settings.mapper.dart';

@MappableClass()
class const LanguageActivationSettings({
  this.enabled = false,
  this.structuralParserEnabled = false,
  this.semanticProviderId,
  this.executablePath,
  this.extraArgs = const <String>[],
}) with LanguageActivationSettingsMappable {
  final bool enabled;
  final bool structuralParserEnabled;
  final String? semanticProviderId;
  final String? executablePath;
  final List<String> extraArgs;

  static const LanguageActivationSettings defaults =
      LanguageActivationSettings();

  factory fromJson(Map<String, Object?> json) =>
      LanguageActivationSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}

@MappableClass()
class const LanguageIntelligenceSettings({
  this.languages = const <String, LanguageActivationSettings>{},
}) with LanguageIntelligenceSettingsMappable {
  /// Persisted by canonical string language id so settings remain stable even
  /// when the runtime [LanguageId] value object evolves.
  final Map<String, LanguageActivationSettings> languages;

  LanguageActivationSettings forLanguage(
    LanguageId language, {
    bool structuralParserDefaultEnabled = false,
  }) =>
      languages[language.value] ??
      LanguageActivationSettings(
        structuralParserEnabled: structuralParserDefaultEnabled,
      );

  LanguageIntelligenceSettings withLanguage(
    LanguageId language,
    LanguageActivationSettings settings,
  ) => LanguageIntelligenceSettings(
    languages: <String, LanguageActivationSettings>{
      ...languages,
      language.value: settings,
    },
  );

  static const LanguageIntelligenceSettings defaults =
      LanguageIntelligenceSettings();

  factory fromJson(Map<String, Object?> json) =>
      LanguageIntelligenceSettingsMapper.fromMap(
        Map<String, dynamic>.from(json),
      );
}
