import 'language_capability.dart';
import 'language_id.dart';

final class LanguageExtensionDescriptor {
  LanguageExtensionDescriptor({
    required this.id,
    required String displayName,
    Iterable<String> fileExtensions = const <String>[],
    Iterable<String> aliases = const <String>[],
    String? parserProviderId,
    Iterable<String> semanticProviderIds = const <String>[],
    String? defaultSemanticProviderId,
    Iterable<LanguageCapability> capabilities = const <LanguageCapability>{},
  }) : displayName = _requireNonEmpty(displayName, 'displayName'),
       fileExtensions = Set<String>.unmodifiable(
         fileExtensions.map(_normalizeExtension),
       ),
       aliases = Set<String>.unmodifiable(aliases.map(_normalizeAlias)),
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
  final String? parserProviderId;
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
