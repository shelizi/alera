part of 'terminal_runtime.dart';

class XtermTerminalRuntime._(
  final TerminalPtySessionFactory _ptySessionFactory,
  var TerminalSettings _settings,
  final ExternalUriLauncher _externalUriLauncher,
  final TerminalRuntimeLaunchInputOwner _launchInputOwner,
  final TerminalSessionCleanup? _terminalSessionCleanup,
) implements TerminalRuntime, TerminalRuntimeSessionOwnerHost {
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

    return XtermTerminalRuntime._(
      ptySessionFactory ?? const DefaultTerminalPtySessionFactory(),
      initialSettings ?? TerminalSettings.defaults,
      externalUriLauncher ?? UrlLauncherExternalUriLauncher(),
      resolvedLaunchInputOwner,
      terminalSessionCleanup,
    );
  }

  final StreamController<TerminalRuntimeExitEvent> _exitController =
      StreamController<TerminalRuntimeExitEvent>.broadcast();
  late final TerminalRuntimeSessionOwner _sessionOwner =
      TerminalRuntimeSessionOwner(this);

  @override
  Stream<TerminalRuntimeExitEvent> get exits => _exitController.stream;

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
      _externalUriLauncher,
      _launchInputOwner,
      owner._handleSessionExit,
      owner._handleVisibilityChanged,
    );
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
