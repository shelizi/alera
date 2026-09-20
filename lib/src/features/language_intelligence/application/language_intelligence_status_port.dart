import '../domain/language_intelligence_settings.dart';
import '../domain/language_intelligence_status.dart';
import '../domain/language_provider_descriptor.dart';
import 'language_server_runtime.dart';

abstract interface class LanguageIntelligenceStatusPort {
  Future<LanguageIntelligenceProviderStatus> resolve({
    required LanguageProviderDescriptor provider,
    required LanguageActivationSettings settings,
  });
}

final class RuntimeLanguageIntelligenceStatusPort
    implements LanguageIntelligenceStatusPort {
  factory RuntimeLanguageIntelligenceStatusPort({
    required LanguageServerRuntimePort runtime,
  }) => RuntimeLanguageIntelligenceStatusPort._(runtime);

  const RuntimeLanguageIntelligenceStatusPort._(this._runtime);

  final LanguageServerRuntimePort _runtime;

  @override
  Future<LanguageIntelligenceProviderStatus> resolve({
    required LanguageProviderDescriptor provider,
    required LanguageActivationSettings settings,
  }) async {
    if (!settings.enabled) {
      return const LanguageIntelligenceProviderStatus.disabled();
    }
    try {
      final resolution = await _runtime.resolveExecutable(
        provider: provider,
        settings: settings,
        target: LanguageServerTarget.localWorkspace,
      );
      return switch (resolution) {
        LanguageServerExecutableResolved(:final executable) =>
          LanguageIntelligenceProviderStatus(
            kind: LanguageIntelligenceStatusKind.ready,
            executable: executable,
          ),
        LanguageServerExecutableMissing(:final reason) =>
          LanguageIntelligenceProviderStatus(
            kind: LanguageIntelligenceStatusKind.missing,
            detail: reason,
          ),
      };
    } on Object catch (error) {
      return LanguageIntelligenceProviderStatus(
        kind: LanguageIntelligenceStatusKind.failed,
        detail: error.toString(),
      );
    }
  }
}
