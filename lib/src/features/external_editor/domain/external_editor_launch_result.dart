enum ExternalEditorLaunchFailureKind { invalidTarget, unavailable, failed }

class const ExternalEditorLaunchResult({
  required final bool ok,
  final ExternalEditorLaunchFailureKind? failureKind,
  final String? message,
});

extension ExternalEditorLaunchResultFactories on ExternalEditorLaunchResult {
  static const opened = ExternalEditorLaunchResult(ok: true);
}

ExternalEditorLaunchResult externalEditorLaunchFailure(
  ExternalEditorLaunchFailureKind kind,
  String message,
) => ExternalEditorLaunchResult(ok: false, failureKind: kind, message: message);

class const ExternalEditorAvailability({
  required final bool available,
  final String? version,
  final String? message,
});
