enum ExternalTerminalLaunchFailureKind { invalidTarget, unavailable, failed }

class const ExternalTerminalOpenRequest({
  required final String workspacePath,
  required final String terminalSessionId,
  required final String title,
});

class const ExternalTerminalLaunchResult({
  required final bool ok,
  final ExternalTerminalLaunchFailureKind? failureKind,
  final String? message,
});

extension ExternalTerminalLaunchResultFactories
    on ExternalTerminalLaunchResult {
  static const opened = ExternalTerminalLaunchResult(ok: true);
}

ExternalTerminalLaunchResult externalTerminalLaunchFailure(
  ExternalTerminalLaunchFailureKind kind,
  String message,
) => ExternalTerminalLaunchResult(
  ok: false,
  failureKind: kind,
  message: message,
);

abstract interface class ExternalTerminalLauncher {
  Future<ExternalTerminalLaunchResult> open(
    ExternalTerminalOpenRequest request,
  );
}
