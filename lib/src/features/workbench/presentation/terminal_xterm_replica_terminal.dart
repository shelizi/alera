import 'dart:async';
import 'dart:math' as math;

import 'package:xterm2/xterm.dart';

import '../domain/terminal_osc52_clipboard.dart';
import 'terminal_xterm_buffer_model.dart';
import 'terminal_xterm_worker.dart';
import 'terminal_xterm_replica_row_writer.dart';

/// UI-isolate xterm state replica fed by [TerminalXtermWorker] deltas.
///
/// This class deliberately never parses PTY output. It only reconstructs the
/// public xterm buffer/state surface that TerminalView reads, allowing Alera to
/// keep the existing renderer while the authoritative parser/model lives in a
/// worker isolate.
final class TerminalXtermReplicaTerminal extends Terminal {
  TerminalXtermReplicaTerminal({
    required int cols,
    required int rows,
    int maxLines = 1000,
    TerminalTargetPlatform platform = TerminalTargetPlatform.unknown,
    Set<int>? wordSeparators,
    void Function(String)? onOutput,
    bool initialFocused = true,
    void Function(bool focused)? onFocusStateChanged,
    bool notificationsEnabled = true,
  }) : _model = TerminalXtermBufferModel(wordSeparators: wordSeparators),
       _onFocusStateChanged = onFocusStateChanged,
       _notificationsEnabled = notificationsEnabled,
       super(
         maxLines: maxLines,
         platform: platform,
         onOutput: onOutput,
         reflowWithHiddenCursor: false,
         preserveOrphanCombiningMarks: true,
         allowITerm2ClipboardCapture: false,
         allowKittyClipboard: false,
         onClipboardQuery: (_) => null,
         clipboardDecoder: decodeTerminalOsc52Payload,
         wordSeparators: wordSeparators,
       ) {
    super.resize(cols, rows);
    super.focusInput(initialFocused);
  }

  final TerminalXtermBufferModel _model;
  final void Function(bool focused)? _onFocusStateChanged;
  var _hasReplicaState = false;
  var _disposed = false;
  bool _notificationsEnabled;
  bool _notificationPending = false;
  bool _applyingReplicaDelta = false;
  int? _requestedViewWidth;
  int? _requestedViewHeight;
  bool _synchronizedUpdateSuppressed = false;
  bool _synchronizedUpdateNotificationPending = false;
  int _synchronizedUpdateGeneration = -1;
  Timer? _synchronizedUpdateReleaseTimer;

  final TerminalXtermReplicaRowWriter _rowWriter =
      TerminalXtermReplicaRowWriter();

  TerminalXtermBufferModel get replicaModel => _model;

  /// The size the view last asked for. It equals [viewWidth] and [viewHeight]
  /// once the worker has applied it and its repaint has arrived.
  int get requestedViewWidth => _requestedViewWidth ?? viewWidth;
  int get requestedViewHeight => _requestedViewHeight ?? viewHeight;

  /// Forwards a view resize to the session instead of reflowing locally.
  ///
  /// The worker owns the buffer and answers a resize with a full repaint, so a
  /// local reflow of the whole scrollback on the UI isolate was pure waste, and
  /// output deltas still in flight at the old size would then resize the
  /// replica straight back. Before a session attaches [onResize] there is no
  /// worker to forward to, so the size applies locally.
  @override
  void resize(
    int newWidth,
    int newHeight, [
    int? pixelWidth,
    int? pixelHeight,
  ]) {
    final forward = onResize;
    if (forward == null || _disposed) {
      super.resize(newWidth, newHeight, pixelWidth, pixelHeight);
      return;
    }
    newWidth = math.max(newWidth, 1);
    newHeight = math.max(newHeight, 1);
    if (newWidth == viewWidth && newHeight == viewHeight) {
      final abandonedRequest = _requestedViewWidth != null;
      _requestedViewWidth = null;
      _requestedViewHeight = null;
      super.resize(newWidth, newHeight, pixelWidth, pixelHeight);
      if (abandonedRequest) {
        // The view went back to the size the worker already has; replace the
        // pending request so the session does not apply the abandoned one.
        forward(newWidth, newHeight, pixelWidth ?? 0, pixelHeight ?? 0);
      }
      return;
    }
    _requestedViewWidth = newWidth;
    _requestedViewHeight = newHeight;
    forward(newWidth, newHeight, pixelWidth ?? 0, pixelHeight ?? 0);
  }

  void _resizeToWorker(int cols, int rows, {required bool reflow}) {
    // Echoing the worker's size through onResize would overwrite a newer
    // pending view size with this one, leaving the PTY at a stale size.
    final forward = onResize;
    final reflowWas = reflowEnabled;
    onResize = null;
    // A full repaint replaces every row, so reflowing them first is wasted.
    reflowEnabled = reflowWas && reflow;
    try {
      super.resize(cols, rows);
    } finally {
      onResize = forward;
      reflowEnabled = reflowWas;
    }
    if (_requestedViewWidth == cols && _requestedViewHeight == rows) {
      _requestedViewWidth = null;
      _requestedViewHeight = null;
    }
  }

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
    if (_applyingReplicaDelta) {
      return;
    }
    if (_synchronizedUpdateSuppressed) {
      _synchronizedUpdateNotificationPending = true;
      return;
    }
    if (!_notificationsEnabled) {
      _notificationPending = true;
      return;
    }
    _notificationPending = false;
    super.notifyListeners();
  }

  void applyBufferDelta(TerminalXtermWorkerBufferDelta delta) {
    final shouldNotify = _bufferDeltaNeedsNotification(delta);
    _updateSynchronizedUpdateState(delta.globalState);
    _applyingReplicaDelta = true;
    try {
      if (super.viewWidth != delta.cols || super.viewHeight != delta.rows) {
        _resizeToWorker(delta.cols, delta.rows, reflow: !delta.fullRepaint);
      }

      final target = super.mainBuffer.lines;
      if (delta.fullRepaint) {
        final rows = List<BufferLine?>.filled(
          delta.bufferLength,
          null,
          growable: false,
        );
        for (final changed in delta.rowDeltas) {
          rows[changed.row] = _rowWriter.buildLine(changed);
        }
        if (rows.any((line) => line == null)) {
          throw StateError(
            'Terminal xterm replica full repaint did not include every row.',
          );
        }
        target.replaceWith(rows.cast<BufferLine>());
      } else {
        if (delta.trimStart > 0) {
          target.trimStart(delta.trimStart);
        }
        for (final changed in delta.rowDeltas) {
          if (changed.row == target.length) {
            target.push(_rowWriter.buildLine(changed));
          } else if (changed.row >= 0 && changed.row < target.length) {
            final line = target[changed.row];
            _rowWriter.applyRowDelta(line, changed);
          } else {
            throw StateError(
              'Terminal xterm replica received invalid row ${changed.row}.',
            );
          }
        }
      }

      if (target.length != delta.bufferLength) {
        throw StateError(
          'Terminal xterm replica has ${target.length} rows; '
          'worker has ${delta.bufferLength}.',
        );
      }

      super.mainBuffer.setCursor(delta.cursorX, delta.cursorY);
      _model.apply(delta);
      super.setKeyboardActionMode(_model.keyboardActionMode);
      super.setBracketedPasteMode(_model.bracketedPaste);
      _hasReplicaState = true;
    } finally {
      _applyingReplicaDelta = false;
    }
    if (_synchronizedUpdateSuppressed) {
      if (shouldNotify) {
        _synchronizedUpdateNotificationPending = true;
      }
      return;
    }
    if (shouldNotify || _synchronizedUpdateNotificationPending) {
      _synchronizedUpdateNotificationPending = false;
      notifyListeners();
    }
  }

  void applyStateDelta(TerminalXtermWorkerStateDelta delta) {
    _updateSynchronizedUpdateState(delta.globalState);
    _model.applyState(delta);
    super.setKeyboardActionMode(_model.keyboardActionMode);
    super.setBracketedPasteMode(_model.bracketedPaste);
    _hasReplicaState = true;
  }

  void _updateSynchronizedUpdateState(TerminalXtermWorkerGlobalState state) {
    if (state.synchronizedUpdate) {
      if (_synchronizedUpdateSuppressed &&
          _synchronizedUpdateGeneration == state.synchronizedUpdateGeneration) {
        return;
      }
      _synchronizedUpdateGeneration = state.synchronizedUpdateGeneration;
      _synchronizedUpdateSuppressed = true;
      _synchronizedUpdateReleaseTimer?.cancel();
      _synchronizedUpdateReleaseTimer = Timer(
        const Duration(milliseconds: 150),
        _releaseSynchronizedUpdate,
      );
      return;
    }

    _synchronizedUpdateGeneration = state.synchronizedUpdateGeneration;
    _synchronizedUpdateSuppressed = false;
    _synchronizedUpdateReleaseTimer?.cancel();
    _synchronizedUpdateReleaseTimer = null;
  }

  bool _bufferDeltaNeedsNotification(TerminalXtermWorkerBufferDelta delta) {
    if (!_hasReplicaState) return true;
    final currentGlobal = _model.globalState;
    final nextGlobal = delta.globalState;
    return delta.fullRepaint ||
        delta.trimStart > 0 ||
        delta.rowDeltas.isNotEmpty ||
        delta.cols != _model.cols ||
        delta.rows != _model.rows ||
        delta.bufferLength != _model.bufferLength ||
        delta.scrollBack != _model.scrollBack ||
        delta.cursorX != _model.cursorX ||
        delta.cursorY != _model.cursorY ||
        delta.cursorVisible != _model.cursorVisible ||
        delta.mouseMode != _model.mouseMode ||
        nextGlobal.isUsingAltBuffer != currentGlobal.isUsingAltBuffer ||
        nextGlobal.reverseDisplay != currentGlobal.reverseDisplay ||
        nextGlobal.cursorType != currentGlobal.cursorType ||
        nextGlobal.cursorBlink != currentGlobal.cursorBlink ||
        nextGlobal.cursorLineHighlight != currentGlobal.cursorLineHighlight ||
        nextGlobal.colorRevision != currentGlobal.colorRevision;
  }

  void _releaseSynchronizedUpdate() {
    _synchronizedUpdateReleaseTimer = null;
    if (_disposed || !_synchronizedUpdateSuppressed) return;
    _synchronizedUpdateSuppressed = false;
    if (!_synchronizedUpdateNotificationPending) return;
    _synchronizedUpdateNotificationPending = false;
    notifyListeners();
  }

  @override
  Buffer get buffer => super.mainBuffer;

  @override
  get lines => super.mainBuffer.lines;

  @override
  bool get isUsingAltBuffer =>
      _hasReplicaState ? _model.isUsingAltBuffer : super.isUsingAltBuffer;

  @override
  bool get reverseDisplayMode =>
      _hasReplicaState ? _model.reverseDisplay : super.reverseDisplayMode;

  @override
  TerminalCursorType? get applicationCursorType => !_hasReplicaState
      ? super.applicationCursorType
      : switch (_model.cursorType) {
          final int value => TerminalCursorType.values[value],
          null => null,
        };

  @override
  bool get cursorBlinkMode =>
      _hasReplicaState ? _model.cursorBlink : super.cursorBlinkMode;

  @override
  bool get cursorVisibleMode =>
      _hasReplicaState ? _model.cursorVisible : super.cursorVisibleMode;

  @override
  bool get cursorKeysMode =>
      _hasReplicaState ? _model.cursorKeys : super.cursorKeysMode;

  @override
  bool get appKeypadMode =>
      _hasReplicaState ? _model.keypadKeys : super.appKeypadMode;

  @override
  bool get lineFeedMode =>
      _hasReplicaState ? _model.lineFeedMode : super.lineFeedMode;

  @override
  bool get ignoreKeypadWithNumLockMode => _hasReplicaState
      ? _model.ignoreKeypadWithNumLockMode
      : super.ignoreKeypadWithNumLockMode;

  @override
  bool get backarrowKeyMode =>
      _hasReplicaState ? _model.backarrowKeyMode : super.backarrowKeyMode;

  @override
  int get kittyKeyboardMode =>
      _hasReplicaState ? _model.kittyKeyboardMode : super.kittyKeyboardMode;

  @override
  int get modifyOtherKeysMode =>
      _hasReplicaState ? _model.modifyOtherKeysMode : super.modifyOtherKeysMode;

  @override
  bool get bracketedPasteMode =>
      _hasReplicaState ? _model.bracketedPaste : super.bracketedPasteMode;

  @override
  bool get reportFocusMode =>
      _hasReplicaState ? _model.focusEvents : super.reportFocusMode;

  @override
  bool get altBufferMouseScrollMode =>
      _hasReplicaState ? _model.altScroll : super.altBufferMouseScrollMode;

  @override
  MouseMode get mouseMode =>
      _hasReplicaState ? MouseMode.values[_model.mouseMode] : super.mouseMode;

  @override
  MouseReportMode get mouseReportMode => _hasReplicaState
      ? MouseReportMode.values[_model.mouseReportMode]
      : super.mouseReportMode;

  @override
  bool get cursorLineHighlightMode => _hasReplicaState
      ? _model.cursorLineHighlight
      : super.cursorLineHighlightMode;

  @override
  bool get mouseShiftCaptureMode =>
      _hasReplicaState ? _model.mouseShiftCapture : super.mouseShiftCaptureMode;

  @override
  bool get altEscPrefixMode =>
      _hasReplicaState ? _model.altEscPrefix : super.altEscPrefixMode;

  @override
  bool get altSendsEscapeMode =>
      _hasReplicaState ? _model.altSendsEscape : super.altSendsEscapeMode;

  @override
  void focusInput(bool focused) {
    _onFocusStateChanged?.call(focused);
    if (!_hasReplicaState) {
      super.focusInput(focused);
      return;
    }
    if (_disposed || !_model.focusEvents) return;
    final escape = String.fromCharCode(27);
    onOutput?.call(focused ? '$escape[I' : '$escape[O');
  }

  @override
  int get colorRevision =>
      _hasReplicaState ? _model.colorRevision : super.colorRevision;

  @override
  Iterable<MapEntry<int, int>> get indexedColorOverrides => _hasReplicaState
      ? _model.indexedColorOverrides.entries
      : super.indexedColorOverrides;

  @override
  Iterable<MapEntry<int, int>> get specialColorOverrides => _hasReplicaState
      ? _model.specialColorOverrides.entries
      : super.specialColorOverrides;

  @override
  int? get foregroundColorOverride => _hasReplicaState
      ? _model.foregroundColorOverride
      : super.foregroundColorOverride;

  @override
  int? get backgroundColorOverride => _hasReplicaState
      ? _model.backgroundColorOverride
      : super.backgroundColorOverride;

  @override
  int? get cursorColorOverride =>
      _hasReplicaState ? _model.cursorColorOverride : super.cursorColorOverride;

  @override
  int? get selectionColorOverride => _hasReplicaState
      ? _model.selectionColorOverride
      : super.selectionColorOverride;

  @override
  int? get selectionForegroundColorOverride => _hasReplicaState
      ? _model.selectionForegroundColorOverride
      : super.selectionForegroundColorOverride;

  @override
  int hyperlinkIdAt(CellOffset position) => _hasReplicaState
      ? _model.hyperlinkIdAt(position.y, position.x)
      : super.hyperlinkIdAt(position);

  @override
  String? hyperlinkAt(CellOffset position) => _hasReplicaState
      ? _model.hyperlinkAt(position.y, position.x)
      : super.hyperlinkAt(position);

  @override
  bool isSemanticPromptLine(int line) => _hasReplicaState
      ? _model.isSemanticPromptLine(line)
      : super.isSemanticPromptLine(line);

  @override
  int? semanticPromptLineBefore(int line) => _hasReplicaState
      ? _model.semanticPromptLineBefore(line)
      : super.semanticPromptLineBefore(line);

  @override
  int? semanticPromptLineAfter(int line) => _hasReplicaState
      ? _model.semanticPromptLineAfter(line)
      : super.semanticPromptLineAfter(line);

  @override
  void dispose() {
    _disposed = true;
    _synchronizedUpdateReleaseTimer?.cancel();
    _synchronizedUpdateReleaseTimer = null;
    super.dispose();
  }
}
