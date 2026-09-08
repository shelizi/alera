enum ExternalEditorLaunchFailureKind { invalidTarget, unavailable, failed }

class const ExternalEditorLaunchResult({
  required this.ok,
  this.failureKind,
  this.message,
});

extension ExternalEditorLaunchResultFactories on ExternalEditorLaunchResult {
  static const opened = ExternalEditorLaunchResult(ok: true);
}

ExternalEditorLaunchResult externalEditorLaunchFailure(
  ExternalEditorLaunchFailureKind kind,
  String message,
) => ExternalEditorLaunchResult(
  ok: false,
  failureKind: kind,
  message: message,
);

class const ExternalEditorAvailability({
  required this.available,
  this.version,
  this.message,
});
