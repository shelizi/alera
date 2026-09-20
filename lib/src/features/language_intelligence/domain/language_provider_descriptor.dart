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
    Iterable<String> executableCandidates = const <String>[],
    Iterable<String> defaultArguments = const <String>[],
  }) : id = _requireId(id),
       languages = Set<LanguageId>.unmodifiable(languages),
       capabilities = Set<LanguageCapability>.unmodifiable(capabilities),
       executableCandidates = List<String>.unmodifiable(
         executableCandidates.map(_normalizeExecutableCandidate),
       ),
       defaultArguments = List<String>.unmodifiable(defaultArguments) {
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
  final List<String> executableCandidates;
  final List<String> defaultArguments;

  static String _requireId(String value) {
    return _requireNonEmpty(value, 'id');
  }

  static String _requireNonEmpty(String value, String name) {
    final normalized = value.trim();
    if (normalized.isEmpty) {
      throw ArgumentError.value(value, name, 'Value must not be empty.');
    }
    return normalized;
  }

  static String _normalizeExecutableCandidate(String value) {
    final normalized = _requireNonEmpty(value, 'executableCandidates');
    if (normalized.contains('/') || normalized.contains(r'\')) {
      throw ArgumentError.value(
        value,
        'executableCandidates',
        'Provider executable candidates must be bare PATH command names.',
      );
    }
    return normalized;
  }
}
