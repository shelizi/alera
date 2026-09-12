part of 'terminal_runtime.dart';

final class TerminalPreparedLaunch {
  const TerminalPreparedLaunch({
    required this.workspaceLaunch,
    required this.interactiveLaunch,
  });

  final GhosttyTerminalShellLaunch workspaceLaunch;
  final GhosttyTerminalShellLaunch interactiveLaunch;

  List<String> get arguments => interactiveLaunch.arguments;
  Map<String, String> get environment =>
      workspaceLaunch.environment ?? const <String, String>{};
  String get interactiveShell => interactiveLaunch.shell;
}

final class TerminalRuntimeLaunchInputOwner {
  TerminalRuntimeLaunchInputOwner({
    required this._shellLaunchesBuilder,
    this._agentHookEnvironmentBuilder,
    this._shellStartupPreparer,
    this._terminalProcessCreated,
    required this._clipboard,
    this._interactionNotice,
    required this._onOsc52Blocked,
  });

  final List<GhosttyTerminalShellLaunch> Function() _shellLaunchesBuilder;
  final TerminalLaunchEnvironmentBuilder? _agentHookEnvironmentBuilder;
  final TerminalShellStartupPreparer? _shellStartupPreparer;
  final TerminalProcessCreated? _terminalProcessCreated;
  final TerminalClipboard _clipboard;
  final void Function(String message, {bool error})? _interactionNotice;
  final VoidCallback _onOsc52Blocked;

  TerminalClipboard get clipboard => _clipboard;

  List<GhosttyTerminalShellLaunch> candidateLaunches() =>
      _shellLaunchesBuilder();

  Future<Map<String, String>?> buildAgentHookEnvironment({
    required String terminalSessionId,
    required String workspaceId,
    required String tabId,
  }) async {
    return _agentHookEnvironmentBuilder?.call(
      terminalSessionId: terminalSessionId,
      workspaceId: workspaceId,
      tabId: tabId,
    );
  }

  Future<TerminalPreparedLaunch> prepareLaunch({
    required GhosttyTerminalShellLaunch launch,
    required String workspacePath,
    required bool resolvedLoginShell,
    Map<String, String>? agentHookEnvironment,
  }) async {
    final sanitizedLaunch = _launchWithSanitizedAgentHookEnvironment(
      resolvedLoginShell ? _launchAsLoginShell(launch) : launch,
      agentHookEnvironment,
    );
    final workspaceAwarePowerShellLaunch =
        _isWindowsPowerShellLaunch(sanitizedLaunch)
            ? _launchInWorkingDirectory(sanitizedLaunch, workspacePath)
            : null;
    final preparedLaunch = await _shellStartupPreparer?.prepare(
      workspaceAwarePowerShellLaunch ?? sanitizedLaunch,
    );
    final interactiveLaunch =
        preparedLaunch ?? workspaceAwarePowerShellLaunch ?? sanitizedLaunch;
    final workspaceLaunch = workspaceAwarePowerShellLaunch == null
        ? _launchInWorkingDirectory(interactiveLaunch, workspacePath)
        : interactiveLaunch;

    return TerminalPreparedLaunch(
      workspaceLaunch: workspaceLaunch,
      interactiveLaunch: interactiveLaunch,
    );
  }

  Future<void> onProcessCreated({
    required String terminalSessionId,
    required TerminalPtySession session,
    required GhosttyTerminalShellLaunch workspaceLaunch,
    required String interactiveShell,
    required String? initialCommand,
    required bool Function() isCurrent,
  }) async {
    if (!isCurrent()) {
      return;
    }
    await _terminalProcessCreated?.call(terminalSessionId);
    if (!isCurrent()) {
      return;
    }
    await _deliverTerminalProcessStartup(
      session: session,
      launch: workspaceLaunch,
      interactiveShell: interactiveShell,
      initialCommand: initialCommand,
      isCurrent: isCurrent,
    );
  }

  void notifyInteraction(String message, {bool error = false}) {
    _interactionNotice?.call(message, error: error);
  }

  void notifyOsc52Blocked() {
    _onOsc52Blocked();
  }

  void storeClipboard({
    required String text,
    required bool allowOsc52Clipboard,
    bool isDisposed = false,
  }) {
    if (isDisposed) {
      return;
    }
    if (!allowOsc52Clipboard) {
      notifyOsc52Blocked();
      return;
    }
    unawaited(
      _clipboard.writeText(text).catchError((Object error) {
        notifyInteraction(
          'Could not copy terminal selection.',
          error: true,
        );
      }),
    );
  }

  Future<void> pasteClipboard({
    required bool isDisposed,
    required void Function(String text) onPasteText,
  }) async {
    String? text;
    try {
      text = await _clipboard.readText();
    } catch (_) {
      // Image-only clipboards can reject text-flavor reads on some platforms.
    }
    if (isDisposed) {
      return;
    }
    if (text != null && text.isNotEmpty) {
      onPasteText(text);
      return;
    }
    try {
      final imagePath = await _clipboard.saveImageAsTempFile();
      if (isDisposed || imagePath == null || imagePath.isEmpty) {
        return;
      }
      onPasteText(sanitizeTerminalImagePastePath(imagePath));
    } catch (error) {
      notifyInteraction('Could not paste clipboard image.', error: true);
    }
  }
}
