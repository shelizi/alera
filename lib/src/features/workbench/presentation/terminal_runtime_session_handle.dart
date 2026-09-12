part of 'terminal_runtime.dart';

class _XtermTerminalSessionHandle(
  final TerminalRuntimeSessionOwner _sessionOwner,
  var Workspace _workspace,
  this._tab,
  final TerminalPtySessionFactory _ptySessionFactory,
  var TerminalSettings _settings,
  final TerminalRuntimeRendererAdapterOwner _rendererAdapterOwner,
  final TerminalRuntimeLaunchInputOwner _launchInputOwner,
  final void Function(TerminalRuntimeExitEvent event) _onExit,
  this._onVisibilityChanged,
) extends TerminalSessionHandle
    with _TerminalSearchSessionSupport, _TerminalSessionCapabilitiesSupport
    implements TerminalRuntimeLaunchInputOwnerHost,
        _TerminalSessionVisibilityHost,
        _TerminalSessionOutputHost {
  this {
    _terminal = _createTerminal();
    _attachTerminal(_terminal);
    _terminalController.addListener(_handleSelectionChanged);
    _decodedOutputSub = _ptyOutputController.stream
        .transform(const Utf8Decoder(allowMalformed: true))
        .listen(_handleTerminalOutput);
  }

  @override
  WorkspaceTabRecord _tab;

  /// Lets the runtime re-run the memory budget when a terminal stops being
  /// visible, which is the only moment a new eviction candidate appears.
  final void Function(_XtermTerminalSessionHandle handle) _onVisibilityChanged;

  @override
  late xterm.Terminal _terminal;
  @override
  final GlobalKey<xterm.TerminalViewState> _terminalViewKey =
      GlobalKey<xterm.TerminalViewState>();
  @override
  final xterm.TerminalController _terminalController = xterm.TerminalController(
    pointerInputs: const .all(),
  );

  /// Survives TerminalSurface dispose/rebuild so scroll position is preserved
  /// when switching tabs and coming back.
  @override
  final ScrollController _scrollController = ScrollController();
  @override
  final FocusNode _focusNode = FocusNode(debugLabel: 'TerminalSession');
  @override
  final StreamController<List<int>> _ptyOutputController =
      StreamController<List<int>>();
  @override
  late final StreamSubscription<String> _decodedOutputSub;

  late final _TerminalSessionOutputPump _pump =
      _TerminalSessionOutputPump(this);
  @override
  TerminalPtySession? _ptySession;
  StreamSubscription<TerminalPtySessionEvent>? _ptySessionSub;
  Timer? _pendingPtyResizeTimer;
  Timer? _selectionCopyTimer;
  Timer? _deferredSubmitEnterTimer;
  _TerminalPtySize? _pendingPtySize;
  int _ptyGeneration = 0;
  @override
  int _startAttempt = 0;
  int? _activePtyGeneration;
  final Set<int> _exitedPtyGenerations = <int>{};
  final Set<int> _suppressedExitPtyGenerations = <int>{};
  @override
  late final _TerminalSessionVisibilityAccounting _visibility =
      _TerminalSessionVisibilityAccounting(this);

  bool _starting = false;
  TerminalSessionOperation? _operation;
  bool _started = false;
  bool _running = false;
  String _title = '';
  @override
  late final ValueNotifier<String> _titleNotifier = ValueNotifier<String>(
    displayTitle,
  );
  String? _errorMessage;
  @override
  final ValueNotifier<TerminalRestoreProgress?> _restoreProgress =
      ValueNotifier<TerminalRestoreProgress?>(null);
  int _restoreGeneration = 0, _restoreTotalChars = 0, _restoreWrittenChars = 0;
  bool _pendingInteractionModeReset = false;
  @override
  int _pointerInputCatchUpChars = 0;
  @override
  bool _pointerInputResumePending = false;
  @override
  int _outputVisibilityGeneration = 0;
  @override
  bool _disposed = false;

  @override
  String get tabId => _tab.id;

  @override
  String get workspaceId => _workspace.id;
  @override
  String get workspacePath => _workspace.path;
  @override
  String get terminalSessionId => _tab.terminalSessionId;

  @override
  ValueListenable<String> get titleListenable => _titleNotifier;

  @override
  bool get isVisible => _visibility.isVisible;

  bool get _outputVisible => _visibility.isOutputVisible;

  @override
  TerminalBufferUsage get bufferUsage =>
      _visibility.estimateUsage(tabId, _terminal);

  @override
  ValueListenable<TerminalRestoreProgress?> get restoreProgress =>
      _restoreProgress;

  // _TerminalSessionVisibilityHost implementation.
  @override
  void onOutputVisibilityChanged() {
    _syncPtyOutputVisibility();
    if (_visibility.isOutputVisible) {
      _pump.scheduleFlush();
    } else {
      _pump.pipeline.cancelDeferredFlush();
    }
  }

  @override
  void onVisibilityChanged() => _onVisibilityChanged(this);

  @override
  void scheduleOutputFlush() => _pump.scheduleFlush();

  @override
  void cancelDeferredFlush() => _pump.pipeline.cancelDeferredFlush();

  @override
  String get displayTitle {
    if (_tab.hasManualTitle) {
      return _tab.title;
    }
    final runtimeTitle = _title.trim();
    if (runtimeTitle.isEmpty || runtimeTitle == 'Terminal') {
      return _tab.title;
    }
    return runtimeTitle;
  }

  @override
  bool get isRunning => _running;

  @override
  bool get isStarting => _starting;

  @override
  TerminalSessionOperation? get operation => _operation;

  @override
  String? get errorMessage => _errorMessage;

  _XtermTerminalSessionHandle sync({
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  }) {
    final metadataChanged =
        _workspace.id != workspace.id ||
        _workspace.path != workspace.path ||
        _tab.id != tab.id ||
        _tab.title != tab.title ||
        _tab.hasManualTitle != tab.hasManualTitle;
    _workspace = workspace;
    _tab = tab;
    _syncTerminalPulseConfiguration(tab);
    _titleNotifier.value = displayTitle;
    if (metadataChanged) {
      // sync() is invoked from build(); defer the notification so listening
      // AnimatedBuilders are not marked dirty during the build phase.
      scheduleMicrotask(() {
        if (!_disposed) {
          notifyListeners();
        }
      });
    }
    return this;
  }

  void applySettings(TerminalSettings settings) {
    _settings = settings;
    if (!settings.clipboardOnSelect) {
      _selectionCopyTimer?.cancel();
      _selectionCopyTimer = null;
    } else {
      _handleSelectionChanged();
    }
    notifyListeners();
  }

  @override
  Future<void> ensureStarted() =>
      _sessionOwner._ensureTerminalSessionStarted(this);

  @override
  Future<void> reconnect() => _sessionOwner._reconnectTerminalSession(this);

  @override
  Future<void> restart() => _sessionOwner._restartTerminalSession(this);

  void _notifySessionListeners() => notifyListeners();

  @override
  TerminalVisibilityLease acquireVisibility() => _visibility.acquireLease();

  @override
  Widget buildView({
    Key? key,
    bool autofocus = false,
    FocusOnKeyEventCallback? onKeyEvent,
  }) {
    return _rendererAdapterOwner._buildInteractiveView(
      key: key,
      session: this,
      autofocus: autofocus,
      onKeyEvent: onKeyEvent,
    );
  }

  xterm.TerminalView _buildTerminalView({
    required bool autofocus,
    FocusOnKeyEventCallback? onKeyEvent,
    required MouseCursor mouseCursor,
    void Function(TapUpDetails details, xterm.CellOffset offset)? onTapUp,
  }) {
    return _rendererAdapterOwner.buildTerminalView(
      terminal: _terminal,
      terminalViewKey: _terminalViewKey,
      controller: _terminalController,
      scrollController: _scrollController,
      focusNode: _focusNode,
      settings: _settings,
      autofocus: autofocus,
      onKeyEvent: onKeyEvent,
      mouseCursor: mouseCursor,
      onTapUp: onTapUp,
      onPaste: _pasteFromClipboard,
      onCopy: _launchInputOwner.clipboard.writeText,
    );
  }

  TerminalLinkRange? _linkAt(xterm.CellOffset offset) {
    return _rendererAdapterOwner.linkAt(terminal: _terminal, offset: offset);
  }

  Future<void> _openLink(Uri uri) {
    return _rendererAdapterOwner.openLink(uri);
  }

  void _handleTitleChanged(String title) {
    _title = title;
    _titleNotifier.value = displayTitle;
  }

  void _handleTerminalInput(String data) {
    _ptySession?.writeBytes(utf8.encode(data));
  }

  void _handleTerminalResize(
    int width,
    int height,
    int pixelWidth,
    int pixelHeight,
  ) {
    _pendingPtySize = _TerminalPtySize(
      cols: width,
      rows: height,
      cellWidthPx: pixelWidth,
      cellHeightPx: pixelHeight,
    );
    _pendingPtyResizeTimer ??= Timer(
      _ptyResizeDebounceDuration,
      _flushPendingPtyResize,
    );
  }

  void _flushPendingPtyResize() {
    _pendingPtyResizeTimer?.cancel();
    _pendingPtyResizeTimer = null;
    final size = _pendingPtySize;
    final session = _ptySession;
    if (_disposed || size == null) {
      _pendingPtySize = null;
      return;
    }
    if (session == null) {
      return;
    }
    _pendingPtySize = null;
    session.resize(size.cols, size.rows, size.cellWidthPx, size.cellHeightPx);
  }

  @override
  Future<void> refreshRendering() {
    return _rendererAdapterOwner.refreshRendering(
      terminalViewKey: _terminalViewKey,
      terminal: _terminal,
      ptySession: _ptySession,
      isDisposed: _disposed,
    );
  }

  Future<void> _pasteFromClipboard() => _pasteTerminalClipboard(this);

  @override
  void _handleSelectionChanged() {
    _handleTerminalSelectionChanged(this);
  }

  @override
  void _notifyInteraction(String message, {bool error = false}) {
    _publishTerminalInteraction(this, message, error: error);
  }

  xterm.Terminal _createTerminal() =>
      _rendererAdapterOwner.createTerminal(settings: _settings);

  void _attachTerminal(xterm.Terminal terminal) {
    _rendererAdapterOwner.attachTerminal(
      terminal,
      onTitleChange: _handleTitleChanged,
      onOutput: _handleTerminalInput,
      onResize: _handleTerminalResize,
      onClipboardStore: (text) => _storeTerminalClipboard(this, text),
    );
  }

  @override
  void _detachTerminal(xterm.Terminal terminal) {
    _rendererAdapterOwner.detachTerminal(terminal);
  }

  void _handleTerminalOutput(String data) => _queueTerminalOutput(data);

  // _TerminalSessionOutputHost implementation.
  @override
  bool get isOutputVisible => _visibility.isOutputVisible;

  @override
  void writeToTerminal(String data) {
    if (data.isEmpty || _disposed) {
      return;
    }
    _terminal.write(data);
  }

  @override
  void advanceRestore(int chars) => _advanceRestore(chars);

  @override
  void finishRestore() => _finishRestore();

  @override
  void advancePointerInputCatchUp(int chars) =>
      _advancePointerInputCatchUp(chars);

  @override
  void discardPointerInputCatchUp({
    required int offset,
    required int chars,
  }) => _discardPointerInputCatchUp(offset: offset, chars: chars);

  void _writeToTerminal(String data) => writeToTerminal(data);

  void _queueTerminalOutput(
    String data, {
    _TerminalOutputSource source = _TerminalOutputSource.live,
  }) => _pump.queue(data, source: source);

  void _flushPendingTerminalOutputFrame({bool force = false}) =>
      _pump.flushFrame(force: force);

  void _flushPendingTerminalOutputNow() => _pump.flushNow();

  void _replaceTerminalWithSnapshot(
    List<int> data, {
    required bool resetInteractionModes,
  }) {
    _rebuildTerminalFromSnapshot(
      data,
      resetInteractionModes: resetInteractionModes,
    );
    notifyListeners();
  }

  @override
  void _clearPendingTerminalOutput() {
    _pump.clearPending();
  }

  Future<void> _stopPtySession({required bool suppressExit}) async {
    await _stopPtySessionWithMode(suppressExit: suppressExit, terminate: true);
  }

  @override
  Future<void> _stopPtySessionWithMode({
    required bool suppressExit,
    required bool terminate,
  }) async {
    _pendingPtyResizeTimer?.cancel();
    _pendingPtyResizeTimer = null;
    _pendingPtySize = null;
    _selectionCopyTimer?.cancel();
    _selectionCopyTimer = null;
    _launchInputOwner.cancelDeferredSubmitEnter(this);
    final generation = _activePtyGeneration;
    if (suppressExit && generation != null) {
      _suppressedExitPtyGenerations.add(generation);
    }
    if (_activePtyGeneration == generation) {
      _activePtyGeneration = null;
    }
    final sub = _ptySessionSub;
    _ptySessionSub = null;
    await sub?.cancel();
    final session = _ptySession;
    _ptySession = null;
    if (terminate) {
      session?.terminate();
    } else {
      session?.dispose();
    }
    _prunePtyGenerationState();
  }

  void _prunePtyGenerationState() {
    final active = _activePtyGeneration;
    _exitedPtyGenerations.removeWhere((generation) => generation != active);
    _suppressedExitPtyGenerations.removeWhere(
      (generation) => generation != active,
    );
  }

  @override
  void requestFocus() {
    _rendererAdapterOwner.requestFocus(_focusNode, isDisposed: _disposed);
  }

  @override
  void pasteText(String text) => _launchInputOwner.pasteText(this, text);

  @override
  Future<bool> submitText(String text) async {
    if (_disposed || text.trim().isEmpty) {
      return false;
    }
    await ensureStarted();
    if (_disposed || !_running || _ptySession == null) {
      return false;
    }
    return _launchInputOwner.submitText(this, text);
  }

  @override
  bool get isDisposed => _disposed;

  @override
  TerminalPtySession? get ptySession => _ptySession;

  @override
  bool get bracketedPasteMode => _terminal.bracketedPasteMode;

  @override
  void pasteToTerminal(String text) => _terminal.paste(text);

  @override
  void deliverInput(String data) => _handleTerminalInput(data);

  @override
  Timer? get deferredSubmitEnterTimer => _deferredSubmitEnterTimer;

  @override
  set deferredSubmitEnterTimer(Timer? timer) =>
      _deferredSubmitEnterTimer = timer;

  void _requestFocusNow() {
    _rendererAdapterOwner.requestFocusNow(_focusNode, isDisposed: _disposed);
  }
}
