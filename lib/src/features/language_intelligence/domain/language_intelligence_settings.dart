import 'language_id.dart';

final class LanguageActivationSettings {
  const LanguageActivationSettings({
    this.enabled = false,
    this.structuralParserEnabled = false,
    this.semanticProviderId,
    this.executablePath,
    this.extraArgs = const <String>[],
  });

  final bool enabled;
  final bool structuralParserEnabled;
  final String? semanticProviderId;
  final String? executablePath;
  final List<String> extraArgs;

  static const LanguageActivationSettings defaults =
      LanguageActivationSettings();
}

final class LanguageIntelligenceSettings {
  factory LanguageIntelligenceSettings({
    Map<LanguageId, LanguageActivationSettings> languages =
        const <LanguageId, LanguageActivationSettings>{},
  }) => LanguageIntelligenceSettings._(
    Map<LanguageId, LanguageActivationSettings>.unmodifiable(languages),
  );

  const LanguageIntelligenceSettings._(this.languages);

  final Map<LanguageId, LanguageActivationSettings> languages;

  LanguageActivationSettings forLanguage(LanguageId language) =>
      languages[language] ?? LanguageActivationSettings.defaults;

  LanguageIntelligenceSettings withLanguage(
    LanguageId language,
    LanguageActivationSettings settings,
  ) => LanguageIntelligenceSettings(
    languages: <LanguageId, LanguageActivationSettings>{
      ...languages,
      language: settings,
    },
  );

  static final LanguageIntelligenceSettings defaults =
      LanguageIntelligenceSettings();
}
