import '../domain/language_capability.dart';
import '../domain/language_extension_descriptor.dart';
import '../domain/language_id.dart';
import '../domain/language_provider_descriptor.dart';

final class LanguageExtensionRegistry {
  final Map<LanguageId, LanguageExtensionDescriptor> _languages =
      <LanguageId, LanguageExtensionDescriptor>{};
  final Map<String, LanguageId> _languageIdsByAlias = <String, LanguageId>{};
  final Map<String, LanguageId> _languageIdsByExtension =
      <String, LanguageId>{};
  final Map<String, LanguageProviderDescriptor> _providers =
      <String, LanguageProviderDescriptor>{};

  Iterable<LanguageExtensionDescriptor> get languages =>
      _languages.values.toList(growable: false);

  Iterable<LanguageProviderDescriptor> get providers =>
      _providers.values.toList(growable: false);

  void registerLanguage(LanguageExtensionDescriptor descriptor) {
    if (_languages.containsKey(descriptor.id)) {
      throw StateError('Language ${descriptor.id} is already registered.');
    }
    if (_languageIdsByAlias.containsKey(descriptor.id.value)) {
      throw StateError(
        'Language id ${descriptor.id} collides with a registered alias.',
      );
    }

    for (final alias in descriptor.aliases) {
      if (_languages.keys.any((id) => id.value == alias) ||
          _languageIdsByAlias.containsKey(alias)) {
        throw StateError('Language alias $alias is already registered.');
      }
    }
    for (final extension in descriptor.fileExtensions) {
      if (_languageIdsByExtension.containsKey(extension)) {
        throw StateError('File extension $extension is already registered.');
      }
    }

    _languages[descriptor.id] = descriptor;
    for (final alias in descriptor.aliases) {
      _languageIdsByAlias[alias] = descriptor.id;
    }
    for (final extension in descriptor.fileExtensions) {
      _languageIdsByExtension[extension] = descriptor.id;
    }
  }

  void registerProvider(LanguageProviderDescriptor descriptor) {
    if (_providers.containsKey(descriptor.id)) {
      throw StateError(
        'Language provider ${descriptor.id} is already registered.',
      );
    }
    for (final language in descriptor.languages) {
      final languageDescriptor = _languages[language];
      if (languageDescriptor == null) {
        throw StateError(
          'Language provider ${descriptor.id} references unregistered language $language.',
        );
      }
      final unsupported = descriptor.capabilities.difference(
        languageDescriptor.capabilities,
      );
      if (unsupported.isNotEmpty) {
        throw StateError(
          'Language provider ${descriptor.id} declares capabilities not listed '
          'by $language: $unsupported.',
        );
      }
      if (descriptor.kind == LanguageProviderKind.structuralParser &&
          languageDescriptor.parserProviderId != descriptor.id) {
        throw StateError(
          'Structural provider ${descriptor.id} is not declared by $language.',
        );
      }
      if (descriptor.kind == LanguageProviderKind.semanticServer &&
          !languageDescriptor.semanticProviderIds.contains(descriptor.id)) {
        throw StateError(
          'Semantic provider ${descriptor.id} is not declared by $language.',
        );
      }
    }

    _providers[descriptor.id] = descriptor;
  }

  LanguageExtensionDescriptor? languageForId(String idOrAlias) {
    final normalized = idOrAlias.trim().toLowerCase();
    if (normalized.isEmpty) {
      return null;
    }
    for (final entry in _languages.entries) {
      if (entry.key.value == normalized) {
        return entry.value;
      }
    }
    final languageId = _languageIdsByAlias[normalized];
    return languageId == null ? null : _languages[languageId];
  }

  LanguageExtensionDescriptor? languageForPath(String path) {
    final extension = _extensionForPath(path);
    final languageId = _languageIdsByExtension[extension];
    return languageId == null ? null : _languages[languageId];
  }

  List<LanguageProviderDescriptor> providersFor(
    LanguageId language,
    LanguageCapability capability,
  ) => List<LanguageProviderDescriptor>.unmodifiable(
    _providers.values.where(
      (descriptor) =>
          descriptor.languages.contains(language) &&
          descriptor.capabilities.contains(capability),
    ),
  );

  LanguageProviderDescriptor? provider(String providerId) =>
      _providers[providerId.trim()];

  static String _extensionForPath(String path) {
    final normalized = path.replaceAll('\\', '/');
    final basename = normalized.substring(normalized.lastIndexOf('/') + 1);
    final dotIndex = basename.lastIndexOf('.');
    if (dotIndex <= 0 || dotIndex == basename.length - 1) {
      return '';
    }
    return basename.substring(dotIndex).toLowerCase();
  }
}
