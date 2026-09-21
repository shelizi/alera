import '../domain/language_provider_descriptor.dart';

sealed class ManagedLanguageServerInstallResult {
  const ManagedLanguageServerInstallResult();
}

final class ManagedLanguageServerInstalled
    extends ManagedLanguageServerInstallResult {
  const ManagedLanguageServerInstalled({
    required this.executable,
    required this.version,
  });

  final String executable;
  final String version;
}

final class ManagedLanguageServerUnavailable
    extends ManagedLanguageServerInstallResult {
  const ManagedLanguageServerUnavailable({required this.reason});

  final String reason;
}

abstract interface class ManagedLanguageServerInstallerPort {
  Future<ManagedLanguageServerInstallResult> ensureInstalled(
    LanguageProviderDescriptor provider,
  );
}
