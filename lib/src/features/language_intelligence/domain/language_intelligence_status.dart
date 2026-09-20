enum LanguageIntelligenceStatusKind { disabled, ready, missing, failed }

final class LanguageIntelligenceProviderStatus {
  const LanguageIntelligenceProviderStatus({
    required this.kind,
    this.detail,
    this.executable,
  });

  const LanguageIntelligenceProviderStatus.disabled()
    : kind = LanguageIntelligenceStatusKind.disabled,
      detail = null,
      executable = null;

  final LanguageIntelligenceStatusKind kind;
  final String? detail;
  final String? executable;
}
