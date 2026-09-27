part of 'terminal_runtime.dart';

class XtermTerminalRuntime._(
  final TerminalPtySessionFactory _ptySessionFactory,
  var TerminalSettings _settings,
  final TerminalRuntimeRendererAdapterOwner _rendererAdapterOwner,
  final TerminalRuntimeLaunchInputOwner _launchInputOwner,
  final TerminalSessionCleanup? _terminalSessionCleanup,
  final bool _parserWorkerEnabled,
  final bool _snapshotHydrationProfilingEnabled,
) implements
    TerminalRuntime,
    TerminalRuntimeSessionOwnerHost,
    TerminalRuntimeUserInputSource {
  factory({
    TerminalPtySessionFactory? ptySessionFactory,
    TerminalSettings? initialSettings,
    ExternalUriLauncher? externalUriLauncher,
    List<GhosttyTerminalShellLaunch> Function()? shellLaunchesBuilder,
    TerminalLaunchEnvironmentBuilder? agentHookEnvironmentBuilder,
    TerminalShellStartupPreparer? shellStartupPreparer,
    TerminalSessionCleanup? terminalSessionCleanup,
    TerminalProcessCreated? terminalProcessCreated,
    TerminalClipboard? terminalClipboard,
    void Function(String message, {bool error})? interactionNotice,
    TerminalRuntimeLaunchInputOwner? launchInputOwner,
    TerminalRuntimeRendererAdapterOwner? rendererAdapterOwner,
    bool parserWorkerEnabled = false,
    bool snapshotHydrationProfilingEnabled = false,
  }) {
    var osc52BlockedNoticeShown = false;
    void notifyOsc52Blocked() {
      if (osc52BlockedNoticeShown) {
        return;
      }
      osc52BlockedNoticeShown = true;
      interactionNotice?.call(
        'Terminal clipboard write blocked. Enable OSC 52 clipboard writes in terminal settings.',
      );
    }

    final resolvedLaunchInputOwner =
        launchInputOwner ??
        TerminalRuntimeLaunchInputOwner(
          shellLaunchesBuilder: shellLaunchesBuilder ?? _terminalShellLaunches,
          agentHookEnvironmentBuilder: agentHookEnvironmentBuilder,
          shellStartupPreparer: shellStartupPreparer,
          terminalProcessCreated: terminalProcessCreated,
          clipboard: terminalClipboard ?? const NativeTerminalClipboard(),
          interactionNotice: interactionNotice,
          onOsc52Blocked: notifyOsc52Blocked,
        );

    final resolvedRendererAdapterOwner =
        rendererAdapterOwner ??
        TerminalRuntimeRendererAdapterOwner(
          externalUriLauncher:
              externalUriLauncher ?? UrlLauncherExternalUriLauncher(),
        );

    return XtermTerminalRuntime._(
      ptySessionFactory ?? const DefaultTerminalPtySessionFactory(),
      initialSettings ?? TerminalSettings.defaults,
      resolvedRendererAdapterOwner,
      resolvedLaunchInputOwner,
      terminalSessionCleanup,
      parserWorkerEnabled,
      snapshotHydrationProfilingEnabled,
    );
  }

  final StreamController<TerminalRuntimeExitEvent> _exitController =
      StreamController<TerminalRuntimeExitEvent>.broadcast();
  final StreamController<String> _userInputController =
      StreamController<String>.broadcast();
  final Map<String, DateTime> _lastUserInputAt = <String, DateTime>{};
  late final TerminalRuntimeSessionOwner _sessionOwner =
      TerminalRuntimeSessionOwner(this);

  TerminalRuntimeRendererAdapterOwner get rendererAdapterOwner =>
      _rendererAdapterOwner;

  @override
  Stream<TerminalRuntimeExitEvent> get exits => _exitController.stream;

  @override
  Stream<String> get userInputWorkspaceIds => _userInputController.stream;

  void updateSettings(TerminalSettings settings) {
    _settings = settings;
    _sessionOwner.applySettings();
  }

  @override
  TerminalSessionHandle sessionFor({
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  }) => _sessionOwner.sessionFor(workspace: workspace, tab: tab);

  @override
  TerminalSessionHandle? peekSession(String tabId) =>
      _sessionOwner.peekSession(tabId);

  @override
  void requestFocus({
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  }) => _sessionOwner.requestFocus(workspace: workspace, tab: tab);

  @override
  void setActiveWorkspace(String? workspaceId) {
    _sessionOwner.setActiveWorkspace(workspaceId);
  }

  /// Parks terminal delivery and frame scheduling while the desktop window is
  /// hidden. The sidecar keeps the PTY and its bounded scrollback alive, then
  /// resynchronises from the delivery cursor when the window returns.
  void setAppForeground(bool foreground) =>
      _sessionOwner.setAppForeground(foreground);

  @override
  TerminalSettings get terminalSettings => _settings;

  @override
  TerminalSessionHandle createSession({
    required TerminalRuntimeSessionOwner owner,
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  }) {
    return _XtermTerminalSessionHandle(
      owner,
      workspace,
      tab,
      _ptySessionFactory,
      _settings,
      _rendererAdapterOwner,
      _launchInputOwner,
      _parserWorkerEnabled,
      _snapshotHydrationProfilingEnabled,
      owner._handleSessionExit,
      owner._handleVisibilityChanged,
      _handleUserInput,
    );
  }

  void _handleUserInput(String workspaceId) {
    final now = DateTime.now();
    final last = _lastUserInputAt[workspaceId];
    if (last != null &&
        now.difference(last) < terminalUserInputActivityInterval) {
      return;
    }
    _lastUserInputAt[workspaceId] = now;
    if (!_userInputController.isClosed) {
      _userInputController.add(workspaceId);
    }
  }

  void _handleSessionExit(TerminalRuntimeExitEvent event) {
    if (!_exitController.isClosed) {
      _exitController.add(event);
    }
  }

  @override
  void forwardSessionExit(TerminalRuntimeExitEvent event) =>
      _handleSessionExit(event);

  @override
  void closeTab(String tabId) => _sessionOwner.closeTab(tabId);

  @override
  void closeWorkspace(String workspaceId) =>
      _sessionOwner.closeWorkspace(workspaceId);

  @override
  void releaseTab(String tabId) => _sessionOwner.releaseTab(tabId);

  @override
  void releaseWorkspace(String workspaceId) =>
      _sessionOwner.releaseWorkspace(workspaceId);

  @override
  void dispose() {
    _sessionOwner.dispose();
    unawaited(_exitController.close());
    unawaited(_userInputController.close());
  }

  @override
  void disposeSession(
    TerminalSessionHandle session, {
    required bool terminatePty,
  }) {
    final xtermSession = session as _XtermTerminalSessionHandle;
    final terminalSessionId = xtermSession.terminalSessionId;
    xtermSession.dispose(terminatePty: terminatePty);
    final cleanup = _terminalSessionCleanup;
    if (terminatePty && cleanup != null) {
      unawaited(
        Future<void>.sync(() => cleanup(terminalSessionId)).catchError((_) {}),
      );
    }
  }
}
