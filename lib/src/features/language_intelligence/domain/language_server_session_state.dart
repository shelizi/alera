enum LanguageServerSessionState {
  disabled,
  available,
  resolvingExecutable,
  starting,
  initializing,
  ready,
  missingExecutable,
  failed,
  stopping,
}

final class LanguageServerSessionSnapshot {
  const LanguageServerSessionSnapshot({
    required this.workspaceId,
    required this.providerId,
    required this.state,
    required this.activeDocumentCount,
    required this.generation,
    required this.restartAttempts,
    this.executable,
    this.lastError,
  });

  factory LanguageServerSessionSnapshot.disabled({
    required String workspaceId,
    String providerId = '',
  }) => LanguageServerSessionSnapshot(
    workspaceId: workspaceId,
    providerId: providerId,
    state: LanguageServerSessionState.disabled,
    activeDocumentCount: 0,
    generation: 0,
    restartAttempts: 0,
  );

  final String workspaceId;
  final String providerId;
  final LanguageServerSessionState state;
  final int activeDocumentCount;
  final int generation;
  final int restartAttempts;
  final String? executable;
  final String? lastError;
}
