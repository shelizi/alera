import 'language_capability.dart';
import 'language_id.dart';

enum LanguageProviderKind { structuralParser, semanticServer }

enum LanguageProviderProcessScope { document, workspace, sharedWorkspaceFamily }

enum LanguageProviderLaunchPolicy { explicit, lazyOnDemand }

enum LanguageExecutableResolutionPolicy { explicitOverrideThenPath }

final class LanguageProviderDescriptor {
  LanguageProviderDescriptor({
    required String id,
    required this.kind,
    required Iterable<LanguageId> languages,
    required Iterable<LanguageCapability> capabilities,
    required this.processScope,
    required this.launchPolicy,
    this.executableResolutionPolicy,
  }) : id = _requireId(id),
       languages = Set<LanguageId>.unmodifiable(languages),
       capabilities = Set<LanguageCapability>.unmodifiable(capabilities) {
    if (this.languages.isEmpty) {
      throw ArgumentError.value(
        languages,
        'languages',
        'Provider must serve a language.',
      );
    }
    if (this.capabilities.isEmpty) {
      throw ArgumentError.value(
        capabilities,
        'capabilities',
        'Provider must expose at least one capability.',
      );
    }
  }

  final String id;
  final LanguageProviderKind kind;
  final Set<LanguageId> languages;
  final Set<LanguageCapability> capabilities;
  final LanguageProviderProcessScope processScope;
  final LanguageProviderLaunchPolicy launchPolicy;
  final LanguageExecutableResolutionPolicy? executableResolutionPolicy;

  static String _requireId(String value) {
    final normalized = value.trim();
    if (normalized.isEmpty) {
      throw ArgumentError.value(value, 'id', 'Provider id must not be empty.');
    }
    return normalized;
  }
}
