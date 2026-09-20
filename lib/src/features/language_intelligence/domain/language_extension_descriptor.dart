import 'language_capability.dart';
import 'language_id.dart';

final class LanguageExtensionDescriptor {
  LanguageExtensionDescriptor({
    required this.id,
    required String displayName,
    Iterable<String> fileExtensions = const <String>[],
    Iterable<String> aliases = const <String>[],
    Map<String, String> syntaxLanguageIdsByExtension = const <String, String>{},
    String? parserProviderId,
    this.structuralParserDefaultEnabled = false,
    Iterable<String> semanticProviderIds = const <String>[],
    String? defaultSemanticProviderId,
    Iterable<LanguageCapability> capabilities = const <LanguageCapability>{},
  }) : displayName = _requireNonEmpty(displayName, 'displayName'),
       fileExtensions = Set<String>.unmodifiable(
         fileExtensions.map(_normalizeExtension),
       ),
       aliases = Set<String>.unmodifiable(aliases.map(_normalizeAlias)),
       syntaxLanguageIdsByExtension = Map<String, String>.unmodifiable(
         syntaxLanguageIdsByExtension.map(
           (extension, languageId) => MapEntry(
             _normalizeExtension(extension),
             _normalizeSyntaxLanguageId(languageId),
           ),
         ),
       ),
       parserProviderId = _normalizeOptionalId(parserProviderId),
       semanticProviderIds = List<String>.unmodifiable(
         semanticProviderIds.map(
           (value) => _requireNonEmpty(value, 'providerId'),
         ),
       ),
       defaultSemanticProviderId = _normalizeOptionalId(
         defaultSemanticProviderId,
       ),
       capabilities = Set<LanguageCapability>.unmodifiable(capabilities) {
    final unknownSyntaxExtensions = this.syntaxLanguageIdsByExtension.keys
        .where((extension) => !this.fileExtensions.contains(extension))
        .toList(growable: false);
    if (unknownSyntaxExtensions.isNotEmpty) {
      throw ArgumentError.value(
        syntaxLanguageIdsByExtension,
        'syntaxLanguageIdsByExtension',
        'Syntax language overrides must reference declared file extensions: '
            '$unknownSyntaxExtensions.',
      );
    }
    final defaultProvider = this.defaultSemanticProviderId;
    if (defaultProvider != null &&
        !this.semanticProviderIds.contains(defaultProvider)) {
      throw ArgumentError.value(
        defaultSemanticProviderId,
        'defaultSemanticProviderId',
        'Default semantic provider must be listed in semanticProviderIds.',
      );
    }
  }

  final LanguageId id;
  final String displayName;
  final Set<String> fileExtensions;
  final Set<String> aliases;
  final Map<String, String> syntaxLanguageIdsByExtension;
  final String? parserProviderId;
  final bool structuralParserDefaultEnabled;
  final List<String> semanticProviderIds;
  final String? defaultSemanticProviderId;
  final Set<LanguageCapability> capabilities;

  static String _normalizeExtension(String value) {
    final normalized = value.trim().toLowerCase();
    if (normalized.isEmpty || normalized == '.') {
      throw ArgumentError.value(value, 'fileExtensions', 'Extension is empty.');
    }
    return normalized.startsWith('.') ? normalized : '.$normalized';
  }

  static String _normalizeAlias(String value) =>
      _requireNonEmpty(value, 'aliases').toLowerCase();

  static String _normalizeSyntaxLanguageId(String value) =>
      _requireNonEmpty(value, 'syntaxLanguageIdsByExtension').toLowerCase();

  static String _requireNonEmpty(String value, String name) {
    final normalized = value.trim();
    if (normalized.isEmpty) {
      throw ArgumentError.value(value, name, 'Value must not be empty.');
    }
    return normalized;
  }

  static String? _normalizeOptionalId(String? value) {
    if (value == null) {
      return null;
    }
    return _requireNonEmpty(value, 'providerId');
  }
}
