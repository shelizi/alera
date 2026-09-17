part of 'terminal_runtime.dart';

/// Owns the presentation adapter layer between the terminal runtime and the
/// GUI renderer (`xterm.Terminal`, `xterm.TerminalView`, themes, styling, and
/// viewport refresh).
final class TerminalRuntimeRendererAdapterOwner {
  TerminalRuntimeRendererAdapterOwner({
    ExternalUriLauncher? externalUriLauncher,
  }) : _externalUriLauncher =
           externalUriLauncher ?? UrlLauncherExternalUriLauncher();

  final ExternalUriLauncher _externalUriLauncher;

  ExternalUriLauncher get externalUriLauncher => _externalUriLauncher;

  /// Instantiates a new [xterm.Terminal] configured from [settings].
  xterm.Terminal createTerminal({
    required TerminalSettings settings,
    bool notificationsEnabled = true,
  }) {
    return _AleraTerminal(
      settings: settings,
      platform: _xtermTargetPlatform,
      wordSeparators: resolveWordSeparators(settings.wordSeparators),
      notificationsEnabled: notificationsEnabled,
    );
  }

  /// Wires the session event listeners to [terminal].
  void attachTerminal(
    xterm.Terminal terminal, {
    required void Function(String title) onTitleChange,
    required void Function(String data) onOutput,
    required void Function(
      int width,
      int height,
      int pixelWidth,
      int pixelHeight,
    )
    onResize,
    required void Function(String text) onClipboardStore,
  }) {
    terminal.onTitleChange = onTitleChange;
    terminal.onOutput = onOutput;
    terminal.onResize = onResize;
    terminal.onClipboardStore = (_, text) => onClipboardStore(text);
  }

  /// Detaches all event listeners from [terminal].
  void detachTerminal(xterm.Terminal terminal) {
    terminal.onTitleChange = null;
    terminal.onOutput = null;
    terminal.onResize = null;
    terminal.onClipboardStore = null;
  }

  /// Resolves the xterm theme from the given [settings].
  xterm.TerminalTheme resolveTheme(TerminalSettings settings) {
    return _resolveXtermTheme(settings);
  }

  /// Resolves the xterm text style from the given [settings].
  xterm.TerminalStyle resolveTextStyle(TerminalSettings settings) {
    return xterm.TerminalStyle(
      fontSize: settings.fontSize,
      fontWeight: settings.fontWeight,
      height: settings.lineHeight,
      fontFamily: resolveFontFamily(settings.fontFamily),
      fontFamilyFallback: _terminalFontFallback,
    );
  }

  /// Resolves the xterm cursor type from [cursorShape].
  xterm.TerminalCursorType resolveCursorType(TerminalCursorShape cursorShape) {
    return cursorShape.toXtermCursorType();
  }

  /// Resolves the font family name.
  String resolveFontFamily(String fontFamily) {
    return _resolveTerminalFontFamily(fontFamily);
  }

  /// Parses rune codepoints from the word separators setting.
  Set<int>? resolveWordSeparators(String? wordSeparators) {
    return _wordSeparatorsFromSettings(wordSeparators);
  }

  /// Builds the underlying [xterm.TerminalView] widget.
  xterm.TerminalView buildTerminalView({
    required xterm.Terminal terminal,
    required GlobalKey<xterm.TerminalViewState> terminalViewKey,
    required xterm.TerminalController controller,
    required ScrollController scrollController,
    required FocusNode focusNode,
    required TerminalSettings settings,
    required bool autofocus,
    required FocusOnKeyEventCallback? onKeyEvent,
    required MouseCursor mouseCursor,
    required void Function(TapUpDetails details, xterm.CellOffset offset)?
    onTapUp,
    required Future<void> Function() onPaste,
    required Future<void> Function(String text)? onCopy,
  }) {
    return xterm.TerminalView(
      terminal,
      key: terminalViewKey,
      shortcuts: xterm.clipboardTerminalShortcuts,
      shiftOverridesMouseReporting: true,
      controller: controller,
      scrollController: scrollController,
      focusNode: focusNode,
      autofocus: autofocus,
      onTapUp: onTapUp,
      onKeyEvent: onKeyEvent,
      mouseCursor: mouseCursor,
      theme: resolveTheme(settings),
      textStyle: resolveTextStyle(settings),
      padding: EdgeInsets.fromLTRB(
        settings.paddingX,
        settings.paddingY,
        settings.paddingX,
        settings.paddingY,
      ),
      cursorType: resolveCursorType(settings.cursorShape),
      cursorBlink: settings.cursorBlink,
      backgroundOpacity: settings.backgroundOpacity,
      hardwareKeyboardOnly: _terminalHardwareKeyboardOnly,
      mouseWheelSensitivity: settings.tuiScrollSensitivity.clamp(1, 10),
      onPaste: onPaste,
      onCopy: onCopy,
    );
  }

  /// Builds the interactive terminal view widget tree.
  Widget _buildInteractiveView({
    Key? key,
    required _XtermTerminalSessionHandle session,
    required bool autofocus,
    FocusOnKeyEventCallback? onKeyEvent,
  }) {
    return _InteractiveTerminalView(
      key: key,
      session: session,
      autofocus: autofocus,
      onKeyEvent: onKeyEvent,
    );
  }

  /// Resolves any hyperlink at [offset] within [terminal].
  TerminalLinkRange? linkAt({
    required xterm.Terminal terminal,
    required xterm.CellOffset offset,
  }) {
    return resolveTerminalLinkAt(terminal: terminal, offset: offset);
  }

  /// Opens the given [uri] via the configured [ExternalUriLauncher].
  Future<void> openLink(Uri uri) {
    return _externalUriLauncher.open(uri);
  }

  /// Immediately requests focus on [focusNode] if not disposed.
  void requestFocusNow(FocusNode focusNode, {required bool isDisposed}) {
    if (isDisposed || !focusNode.canRequestFocus) {
      return;
    }
    focusNode.requestFocus();
  }

  /// Defers focus request to the next frame callback.
  void requestFocus(FocusNode focusNode, {required bool isDisposed}) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      requestFocusNow(focusNode, isDisposed: isDisposed);
    });
  }

  /// Pulses the mounted PTY viewport and schedules a one-shot repaint without
  /// replacing the emulator or PTY session.
  Future<void> refreshRendering({
    required GlobalKey<xterm.TerminalViewState> terminalViewKey,
    required xterm.Terminal terminal,
    required TerminalPtySession? ptySession,
    required bool isDisposed,
  }) async {
    if (isDisposed) {
      return;
    }
    final session = ptySession;
    if (session == null) {
      return;
    }
    final viewState = terminalViewKey.currentState;
    if (viewState == null) {
      return;
    }
    final renderTerminal = viewState.renderTerminal;
    if (!renderTerminal.attached ||
        !renderTerminal.hasSize ||
        renderTerminal.size.isEmpty) {
      return;
    }
    final cellSize = renderTerminal.cellSize;
    await session.refreshViewport(
      terminal.viewWidth,
      terminal.viewHeight,
      cellSize.width.round(),
      cellSize.height.round(),
    );
    if (isDisposed ||
        !identical(terminalViewKey.currentState, viewState) ||
        !renderTerminal.attached) {
      return;
    }
    renderTerminal.markNeedsLayout();
    renderTerminal.markNeedsPaint();
  }
}

/// xterm parsing and buffer mutation must continue while an inactive terminal
/// catches up, but no visible renderer needs one listener dispatch per chunk.
/// Keep a dirty bit while hidden and emit a single notification when the
/// session becomes visible again. Parser side-effects such as title changes,
/// OSC callbacks, and mode changes still run because only Observable delivery
/// is gated.
final class _AleraTerminal extends xterm.Terminal {
  _AleraTerminal({
    required TerminalSettings settings,
    required xterm.TerminalTargetPlatform platform,
    required Set<int>? wordSeparators,
    required bool notificationsEnabled,
  }) : _notificationsEnabled = notificationsEnabled,
       super(
         reflowWithHiddenCursor: false,
         preserveOrphanCombiningMarks: true,
         allowITerm2ClipboardCapture: false,
         allowKittyClipboard: false,
         // An unset callback lets TerminalView install its system clipboard reader.
         onClipboardQuery: (_) => null,
         clipboardDecoder: decodeTerminalOsc52Payload,
         maxLines: settings.scrollbackLines,
         platform: platform,
         wordSeparators: wordSeparators,
       );

  bool _notificationsEnabled;
  bool _notificationPending = false;

  void setNotificationsEnabled(bool enabled, {bool flushPending = true}) {
    _notificationsEnabled = enabled;
    if (enabled && flushPending) {
      flushPendingNotification();
    }
  }

  void flushPendingNotification() {
    if (!_notificationsEnabled || !_notificationPending) {
      return;
    }
    _notificationPending = false;
    super.notifyListeners();
  }

  @override
  void notifyListeners() {
    if (!_notificationsEnabled) {
      _notificationPending = true;
      return;
    }
    _notificationPending = false;
    super.notifyListeners();
  }
}
