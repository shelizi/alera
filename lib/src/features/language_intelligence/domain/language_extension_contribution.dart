import 'language_extension_descriptor.dart';
import 'language_provider_descriptor.dart';

final class LanguageExtensionContribution {
  LanguageExtensionContribution({
    required Iterable<LanguageExtensionDescriptor> languages,
    required Iterable<LanguageProviderDescriptor> providers,
  }) : languages = List<LanguageExtensionDescriptor>.unmodifiable(languages),
       providers = List<LanguageProviderDescriptor>.unmodifiable(providers) {
    if (this.languages.isEmpty) {
      throw ArgumentError.value(
        languages,
        'languages',
        'An extension contribution must register at least one language.',
      );
    }
  }

  final List<LanguageExtensionDescriptor> languages;
  final List<LanguageProviderDescriptor> providers;
}
