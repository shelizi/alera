import 'dart:isolate';
import 'dart:typed_data';

import 'package:xterm2/xterm.dart';

import '../domain/terminal_osc52_clipboard.dart';

const String _workerReady = 'ready';
const String _workerWrite = 'write';
const String _workerWriteDelta = 'writeDelta';
const String _workerWriteBufferDelta = 'writeBufferDelta';
const String _workerResize = 'resize';
const String _workerResizeDelta = 'resizeDelta';
const String _workerResizeBufferDelta = 'resizeBufferDelta';
const String _workerKeyInput = 'keyInput';
const String _workerTextInput = 'textInput';
const String _workerPaste = 'paste';
const String _workerFocusInput = 'focusInput';
const String _workerMouseInput = 'mouseInput';
const String _workerClose = 'close';
const String _workerError = 'error';
const String _effectTitleChanged = 'titleChanged';
const String _effectBell = 'bell';
const String _effectPtyWrite = 'ptyWrite';
const String _effectClipboardStore = 'clipboardStore';

sealed class TerminalXtermWorkerEffect {
  const TerminalXtermWorkerEffect();
}

final class TerminalXtermWorkerTitleChanged extends TerminalXtermWorkerEffect {
  const TerminalXtermWorkerTitleChanged(this.title);

  final String title;
}

final class TerminalXtermWorkerBell extends TerminalXtermWorkerEffect {
  const TerminalXtermWorkerBell();
}

final class TerminalXtermWorkerPtyWrite extends TerminalXtermWorkerEffect {
  const TerminalXtermWorkerPtyWrite(this.data);

  final String data;
}

final class TerminalXtermWorkerClipboardStore
    extends TerminalXtermWorkerEffect {
  const TerminalXtermWorkerClipboardStore({
    required this.selector,
    required this.text,
  });

  final String selector;
  final String text;
}

TerminalXtermWorkerEffect _decodeWorkerEffect(List<Object?> message) {
  return switch (message.first) {
    _effectTitleChanged => TerminalXtermWorkerTitleChanged(
      message[1]! as String,
    ),
    _effectBell => const TerminalXtermWorkerBell(),
    _effectPtyWrite => TerminalXtermWorkerPtyWrite(message[1]! as String),
    _effectClipboardStore => TerminalXtermWorkerClipboardStore(
      selector: message[1]! as String,
      text: message[2]! as String,
    ),
    final Object? tag => throw StateError('Unknown xterm worker effect: $tag'),
  };
}

List<TerminalXtermWorkerEffect> _decodeWorkerEffects(Object? raw) {
  return <TerminalXtermWorkerEffect>[
    for (final effect in raw! as List)
      _decodeWorkerEffect(List<Object?>.from(effect as List)),
  ];
}

final class TerminalXtermWorkerActionResult {
  const TerminalXtermWorkerActionResult({
    required this.revision,
    required this.handled,
    required this.effects,
  });

  factory TerminalXtermWorkerActionResult._fromMessage(List<Object?> message) {
    return TerminalXtermWorkerActionResult(
      revision: message[0]! as int,
      handled: message[1] as bool?,
      effects: _decodeWorkerEffects(message[2]),
    );
  }

  final int revision;
  final bool? handled;
  final List<TerminalXtermWorkerEffect> effects;
}

final class TerminalXtermWorkerSnapshot {
  const TerminalXtermWorkerSnapshot({
    required this.viewportRows,
    required this.cursorX,
    required this.cursorY,
    required this.cursorVisible,
    required this.cursorKeys,
    required this.keypadKeys,
    required this.bracketedPaste,
    required this.focusEvents,
    required this.altScroll,
    required this.mouseMode,
    required this.mouseReportMode,
    required this.scrollBack,
    required this.effects,
  });

  factory TerminalXtermWorkerSnapshot._fromMessage(List<Object?> message) {
    return TerminalXtermWorkerSnapshot(
      viewportRows: List<String>.from(message[0]! as List),
      cursorX: message[1]! as int,
      cursorY: message[2]! as int,
      cursorVisible: message[3]! as bool,
      cursorKeys: message[4]! as bool,
      keypadKeys: message[5]! as bool,
      bracketedPaste: message[6]! as bool,
      focusEvents: message[7]! as bool,
      altScroll: message[8]! as bool,
      mouseMode: message[9]! as int,
      mouseReportMode: message[10]! as int,
      scrollBack: message[11]! as int,
      effects: _decodeWorkerEffects(message[12]),
    );
  }

  final List<String> viewportRows;
  final int cursorX;
  final int cursorY;
  final bool cursorVisible;
  final bool cursorKeys;
  final bool keypadKeys;
  final bool bracketedPaste;
  final bool focusEvents;
  final bool altScroll;
  final int mouseMode;
  final int mouseReportMode;
  final int scrollBack;
  final List<TerminalXtermWorkerEffect> effects;
}

final class TerminalXtermWorkerRenderCell {
  const TerminalXtermWorkerRenderCell({
    required this.text,
    required this.width,
    required this.foreground,
    required this.background,
    required this.attributes,
    required this.underlineColor,
    required this.content,
    required this.hyperlinkId,
    required this.semanticAttributes,
  });

  factory TerminalXtermWorkerRenderCell._fromMessage(List<Object?> message) {
    return TerminalXtermWorkerRenderCell(
      text: message[0]! as String,
      width: message[1]! as int,
      foreground: message[2]! as int,
      background: message[3]! as int,
      attributes: message[4]! as int,
      underlineColor: message[5]! as int,
      content: message[6]! as int,
      hyperlinkId: message[7]! as int,
      semanticAttributes: message[8]! as int,
    );
  }

  final String text;
  final int width;
  final int foreground;
  final int background;
  final int attributes;
  final int underlineColor;
  final int content;
  final int hyperlinkId;
  final int semanticAttributes;
}

final class TerminalXtermWorkerRowDelta {
  const TerminalXtermWorkerRowDelta({
    required this.row,
    required this.text,
    required this.cells,
  });

  factory TerminalXtermWorkerRowDelta._fromMessage(List<Object?> message) {
    return TerminalXtermWorkerRowDelta(
      row: message[0]! as int,
      text: message[1]! as String,
      cells: <TerminalXtermWorkerRenderCell>[
        for (final raw in message[2]! as List)
          TerminalXtermWorkerRenderCell._fromMessage(
            List<Object?>.from(raw as List),
          ),
      ],
    );
  }

  final int row;
  final String text;
  final List<TerminalXtermWorkerRenderCell> cells;
}

final class TerminalXtermWorkerDelta {
  const TerminalXtermWorkerDelta({
    required this.revision,
    required this.fullRepaint,
    required this.cols,
    required this.rows,
    required this.rowDeltas,
    required this.cursorX,
    required this.cursorY,
    required this.cursorVisible,
    required this.cursorKeys,
    required this.keypadKeys,
    required this.bracketedPaste,
    required this.focusEvents,
    required this.altScroll,
    required this.mouseMode,
    required this.mouseReportMode,
    required this.scrollBack,
    required this.effects,
  });

  factory TerminalXtermWorkerDelta._fromMessage(List<Object?> message) {
    return TerminalXtermWorkerDelta(
      revision: message[16]! as int,
      fullRepaint: message[0]! as bool,
      cols: message[1]! as int,
      rows: message[2]! as int,
      rowDeltas: <TerminalXtermWorkerRowDelta>[
        for (final raw in message[3]! as List)
          TerminalXtermWorkerRowDelta._fromMessage(
            List<Object?>.from(raw as List),
          ),
      ],
      cursorX: message[4]! as int,
      cursorY: message[5]! as int,
      cursorVisible: message[6]! as bool,
      cursorKeys: message[7]! as bool,
      keypadKeys: message[8]! as bool,
      bracketedPaste: message[9]! as bool,
      focusEvents: message[10]! as bool,
      altScroll: message[11]! as bool,
      mouseMode: message[12]! as int,
      mouseReportMode: message[13]! as int,
      scrollBack: message[14]! as int,
      effects: _decodeWorkerEffects(message[15]),
    );
  }

  final int revision;
  final bool fullRepaint;
  final int cols;
  final int rows;
  final List<TerminalXtermWorkerRowDelta> rowDeltas;
  final int cursorX;
  final int cursorY;
  final bool cursorVisible;
  final bool cursorKeys;
  final bool keypadKeys;
  final bool bracketedPaste;
  final bool focusEvents;
  final bool altScroll;
  final int mouseMode;
  final int mouseReportMode;
  final int scrollBack;
  final List<TerminalXtermWorkerEffect> effects;
}

Map<int, int> _decodeIntMap(Object? raw) {
  return <int, int>{
    for (final pair in raw! as List) (pair as List)[0]! as int: pair[1]! as int,
  };
}

Map<int, String> _decodeStringMap(Object? raw) {
  return <int, String>{
    for (final pair in raw! as List)
      (pair as List)[0]! as int: pair[1]! as String,
  };
}

final class TerminalXtermWorkerGlobalState {
  const TerminalXtermWorkerGlobalState({
    required this.isUsingAltBuffer,
    required this.reverseDisplay,
    required this.cursorType,
    required this.cursorBlink,
    required this.cursorLineHighlight,
    required this.mouseShiftCapture,
    required this.altEscPrefix,
    required this.altSendsEscape,
    required this.colorRevision,
    required this.indexedColorOverrides,
    required this.specialColorOverrides,
    required this.foregroundColorOverride,
    required this.backgroundColorOverride,
    required this.cursorColorOverride,
    required this.selectionColorOverride,
    required this.selectionForegroundColorOverride,
  });

  factory TerminalXtermWorkerGlobalState._fromMessage(List<Object?> message) {
    return TerminalXtermWorkerGlobalState(
      isUsingAltBuffer: message[0]! as bool,
      reverseDisplay: message[1]! as bool,
      cursorType: message[2] as int?,
      cursorBlink: message[3]! as bool,
      cursorLineHighlight: message[4]! as bool,
      mouseShiftCapture: message[5]! as bool,
      altEscPrefix: message[6]! as bool,
      altSendsEscape: message[7]! as bool,
      colorRevision: message[8]! as int,
      indexedColorOverrides: _decodeIntMap(message[9]),
      specialColorOverrides: _decodeIntMap(message[10]),
      foregroundColorOverride: message[11] as int?,
      backgroundColorOverride: message[12] as int?,
      cursorColorOverride: message[13] as int?,
      selectionColorOverride: message[14] as int?,
      selectionForegroundColorOverride: message[15] as int?,
    );
  }

  final bool isUsingAltBuffer;
  final bool reverseDisplay;
  final int? cursorType;
  final bool cursorBlink;
  final bool cursorLineHighlight;
  final bool mouseShiftCapture;
  final bool altEscPrefix;
  final bool altSendsEscape;
  final int colorRevision;
  final Map<int, int> indexedColorOverrides;
  final Map<int, int> specialColorOverrides;
  final int? foregroundColorOverride;
  final int? backgroundColorOverride;
  final int? cursorColorOverride;
  final int? selectionColorOverride;
  final int? selectionForegroundColorOverride;
}

final class TerminalXtermWorkerBufferDelta {
  const TerminalXtermWorkerBufferDelta({
    required this.revision,
    required this.fullRepaint,
    required this.cols,
    required this.rows,
    required this.bufferLength,
    required this.scrollBack,
    required this.trimStart,
    required this.rowDeltas,
    required this.cursorX,
    required this.cursorY,
    required this.cursorVisible,
    required this.cursorKeys,
    required this.keypadKeys,
    required this.bracketedPaste,
    required this.focusEvents,
    required this.altScroll,
    required this.mouseMode,
    required this.mouseReportMode,
    required this.effects,
    required this.globalState,
    required this.hyperlinkUpdates,
  });

  factory TerminalXtermWorkerBufferDelta._fromMessage(List<Object?> message) {
    return TerminalXtermWorkerBufferDelta(
      fullRepaint: message[0]! as bool,
      cols: message[1]! as int,
      rows: message[2]! as int,
      bufferLength: message[3]! as int,
      scrollBack: message[4]! as int,
      trimStart: message[5]! as int,
      rowDeltas: <TerminalXtermWorkerRowDelta>[
        for (final raw in message[6]! as List)
          TerminalXtermWorkerRowDelta._fromMessage(
            List<Object?>.from(raw as List),
          ),
      ],
      cursorX: message[7]! as int,
      cursorY: message[8]! as int,
      cursorVisible: message[9]! as bool,
      cursorKeys: message[10]! as bool,
      keypadKeys: message[11]! as bool,
      bracketedPaste: message[12]! as bool,
      focusEvents: message[13]! as bool,
      altScroll: message[14]! as bool,
      mouseMode: message[15]! as int,
      mouseReportMode: message[16]! as int,
      effects: _decodeWorkerEffects(message[17]),
      revision: message[18]! as int,
      globalState: TerminalXtermWorkerGlobalState._fromMessage(
        List<Object?>.from(message[19]! as List),
      ),
      hyperlinkUpdates: _decodeStringMap(message[20]),
    );
  }

  final int revision;
  final bool fullRepaint;
  final int cols;
  final int rows;
  final int bufferLength;
  final int scrollBack;
  final int trimStart;
  final List<TerminalXtermWorkerRowDelta> rowDeltas;
  final int cursorX;
  final int cursorY;
  final bool cursorVisible;
  final bool cursorKeys;
  final bool keypadKeys;
  final bool bracketedPaste;
  final bool focusEvents;
  final bool altScroll;
  final int mouseMode;
  final int mouseReportMode;
  final List<TerminalXtermWorkerEffect> effects;
  final TerminalXtermWorkerGlobalState globalState;
  final Map<int, String> hyperlinkUpdates;
}

final class TerminalXtermWorker {
  TerminalXtermWorker._(this._commands, this._isolate);

  final SendPort _commands;
  final Isolate _isolate;
  var _closed = false;

  static Future<TerminalXtermWorker> start({
    required int cols,
    required int rows,
    int maxLines = 1000,
    TerminalTargetPlatform platform = TerminalTargetPlatform.unknown,
    Set<int>? wordSeparators,
  }) async {
    final ready = ReceivePort();
    final errors = ReceivePort();
    final isolate = await Isolate.spawn<List<Object?>>(
      terminalXtermWorkerMain,
      <Object?>[
        ready.sendPort,
        cols,
        rows,
        maxLines,
        platform.index,
        wordSeparators?.toList(growable: false),
      ],
      onError: errors.sendPort,
    );

    try {
      final first = await Future.any<Object?>(<Future<Object?>>[
        ready.first,
        errors.first,
      ]);
      if (first is! List<Object?> ||
          first.isEmpty ||
          first[0] != _workerReady ||
          first.length < 2 ||
          first[1] is! SendPort) {
        throw StateError('Terminal xterm worker failed to start: $first');
      }
      return TerminalXtermWorker._(first[1]! as SendPort, isolate);
    } catch (_) {
      isolate.kill(priority: Isolate.immediate);
      rethrow;
    } finally {
      ready.close();
      errors.close();
    }
  }

  Future<TerminalXtermWorkerSnapshot> write(String data) {
    return _request(<Object?>[_workerWrite, data]);
  }

  Future<TerminalXtermWorkerDelta> writeDelta(String data) async {
    return TerminalXtermWorkerDelta._fromMessage(
      await _requestRaw(<Object?>[_workerWriteDelta, data]),
    );
  }

  Future<TerminalXtermWorkerBufferDelta> writeBufferDelta(String data) async {
    return TerminalXtermWorkerBufferDelta._fromMessage(
      await _requestRaw(<Object?>[_workerWriteBufferDelta, data]),
    );
  }

  Future<TerminalXtermWorkerSnapshot> resize({
    required int cols,
    required int rows,
  }) {
    return _request(<Object?>[_workerResize, cols, rows]);
  }

  Future<TerminalXtermWorkerDelta> resizeDelta({
    required int cols,
    required int rows,
  }) async {
    return TerminalXtermWorkerDelta._fromMessage(
      await _requestRaw(<Object?>[_workerResizeDelta, cols, rows]),
    );
  }

  Future<TerminalXtermWorkerBufferDelta> resizeBufferDelta({
    required int cols,
    required int rows,
  }) async {
    return TerminalXtermWorkerBufferDelta._fromMessage(
      await _requestRaw(<Object?>[_workerResizeBufferDelta, cols, rows]),
    );
  }

  Future<TerminalXtermWorkerActionResult> keyInput(
    TerminalKey key, {
    bool shift = false,
    bool alt = false,
    bool ctrl = false,
    bool superKey = false,
    bool capsLock = false,
    bool numLock = false,
    TerminalKeyEventType type = TerminalKeyEventType.press,
    String? text,
  }) async {
    return TerminalXtermWorkerActionResult._fromMessage(
      await _requestRaw(<Object?>[
        _workerKeyInput,
        key.index,
        shift,
        alt,
        ctrl,
        superKey,
        capsLock,
        numLock,
        type.index,
        text,
      ]),
    );
  }

  Future<TerminalXtermWorkerActionResult> textInput(String text) async {
    return TerminalXtermWorkerActionResult._fromMessage(
      await _requestRaw(<Object?>[_workerTextInput, text]),
    );
  }

  Future<TerminalXtermWorkerActionResult> paste(String text) async {
    return TerminalXtermWorkerActionResult._fromMessage(
      await _requestRaw(<Object?>[_workerPaste, text]),
    );
  }

  Future<TerminalXtermWorkerActionResult> focusInput(bool focused) async {
    return TerminalXtermWorkerActionResult._fromMessage(
      await _requestRaw(<Object?>[_workerFocusInput, focused]),
    );
  }

  Future<TerminalXtermWorkerActionResult> mouseInput(
    TerminalMouseButton button,
    TerminalMouseButtonState buttonState,
    CellOffset position, {
    bool motion = false,
    TerminalMouseModifiers modifiers = TerminalMouseModifiers.none,
    CellOffset? pixelPosition,
  }) async {
    return TerminalXtermWorkerActionResult._fromMessage(
      await _requestRaw(<Object?>[
        _workerMouseInput,
        button.index,
        buttonState.index,
        position.x,
        position.y,
        motion,
        modifiers.shift,
        modifiers.alt,
        modifiers.control,
        pixelPosition?.x,
        pixelPosition?.y,
      ]),
    );
  }

  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    try {
      await _requestRaw(const <Object?>[_workerClose], allowClosed: true);
    } finally {
      _isolate.kill(priority: Isolate.immediate);
    }
  }

  Future<TerminalXtermWorkerSnapshot> _request(List<Object?> command) async {
    return TerminalXtermWorkerSnapshot._fromMessage(await _requestRaw(command));
  }

  Future<List<Object?>> _requestRaw(
    List<Object?> command, {
    bool allowClosed = false,
  }) async {
    if (_closed && !allowClosed) {
      throw StateError('Terminal xterm worker is closed.');
    }
    final reply = ReceivePort();
    try {
      _commands.send(<Object?>[reply.sendPort, ...command]);
      final message = await reply.first;
      if (message is! List<Object?>) {
        throw StateError('Unexpected Terminal xterm worker reply: $message');
      }
      if (message.isNotEmpty && message.first == _workerError) {
        throw StateError(message.length > 1 ? '${message[1]}' : 'Worker error');
      }
      return message;
    } finally {
      reply.close();
    }
  }
}

final class _TerminalXtermWorkerRowCache {
  const _TerminalXtermWorkerRowCache({
    required this.cells,
    required this.combiningCharacters,
    required this.isWrapped,
  });

  factory _TerminalXtermWorkerRowCache.capture(BufferLine line) {
    final cells = Uint32List(line.length * 5);
    for (var column = 0; column < line.length; column++) {
      final offset = column * 5;
      cells[offset] = line.getForeground(column);
      cells[offset + 1] = line.getBackground(column);
      cells[offset + 2] = line.getAttributes(column);
      cells[offset + 3] = line.getContent(column);
      cells[offset + 4] = line.getUnderlineColor(column);
    }
    return _TerminalXtermWorkerRowCache(
      cells: cells,
      combiningCharacters: line.hasCombiningCharacters
          ? <String?>[
              for (var column = 0; column < line.length; column++)
                line.getCombiningCharacters(column),
            ]
          : null,
      isWrapped: line.isWrapped,
    );
  }

  final Uint32List cells;
  final List<String?>? combiningCharacters;
  final bool isWrapped;

  bool matches(BufferLine line) {
    if (isWrapped != line.isWrapped || cells.length != line.length * 5) {
      return false;
    }
    for (var column = 0; column < line.length; column++) {
      final offset = column * 5;
      if (cells[offset] != line.getForeground(column) ||
          cells[offset + 1] != line.getBackground(column) ||
          cells[offset + 2] != line.getAttributes(column) ||
          cells[offset + 3] != line.getContent(column) ||
          cells[offset + 4] != line.getUnderlineColor(column)) {
        return false;
      }
    }

    final currentHasCombining = line.hasCombiningCharacters;
    if (currentHasCombining != (combiningCharacters != null)) {
      return false;
    }
    final cachedCombining = combiningCharacters;
    if (cachedCombining != null) {
      for (var column = 0; column < line.length; column++) {
        if (cachedCombining[column] != line.getCombiningCharacters(column)) {
          return false;
        }
      }
    }
    return true;
  }
}

void terminalXtermWorkerMain(List<Object?> initialization) {
  final owner = initialization[0]! as SendPort;
  final cols = initialization[1]! as int;
  final rows = initialization[2]! as int;
  final maxLines = initialization[3]! as int;
  final platform = TerminalTargetPlatform.values[initialization[4]! as int];
  final rawWordSeparators = initialization[5] as List?;
  final wordSeparators = rawWordSeparators == null
      ? null
      : Set<int>.from(rawWordSeparators);
  final commands = ReceivePort();
  final effects = <List<Object?>>[];
  final terminal = Terminal(
    maxLines: maxLines,
    reflowWithHiddenCursor: false,
    preserveOrphanCombiningMarks: true,
    allowITerm2ClipboardCapture: false,
    allowKittyClipboard: false,
    onClipboardQuery: (_) => null,
    clipboardDecoder: decodeTerminalOsc52Payload,
    platform: platform,
    wordSeparators: wordSeparators,
    onTitleChange: (title) =>
        effects.add(<Object?>[_effectTitleChanged, title]),
    onBell: () => effects.add(const <Object?>[_effectBell]),
    onOutput: (value) => effects.add(<Object?>[_effectPtyWrite, value]),
    onClipboardStore: (selector, text) =>
        effects.add(<Object?>[_effectClipboardStore, selector, text]),
  )..resize(cols, rows);
  var revision = 0;
  List<_TerminalXtermWorkerRowCache>? viewportCache;
  var cachedCols = 0;
  var cachedRows = 0;
  List<BufferLine>? bufferLineRefs;
  List<_TerminalXtermWorkerRowCache>? bufferLineCaches;

  List<Object?> renderCellMessage(
    BufferLine line,
    int column, {
    int? bufferRow,
    Map<int, String>? hyperlinkUpdates,
  }) {
    final codePoint = line.getCodePoint(column);
    final combining = line.getCombiningCharacters(column);
    final hyperlinkId = line.getHyperlinkId(column);
    if (hyperlinkId != 0 &&
        bufferRow != null &&
        hyperlinkUpdates != null &&
        !hyperlinkUpdates.containsKey(hyperlinkId)) {
      final uri = terminal.hyperlinkAt(CellOffset(column, bufferRow));
      if (uri != null) {
        hyperlinkUpdates[hyperlinkId] = uri;
      }
    }
    final text = switch (codePoint) {
      0 => '',
      _ => String.fromCharCode(codePoint) + (combining ?? ''),
    };
    return <Object?>[
      text,
      line.getWidth(column),
      line.getForeground(column),
      line.getBackground(column),
      line.getAttributes(column),
      line.getUnderlineColor(column),
      line.getContent(column),
      hyperlinkId,
      line.getSemanticContent(column),
    ];
  }

  List<Object?> rowMessage(
    int row,
    BufferLine line, {
    int? bufferRow,
    Map<int, String>? hyperlinkUpdates,
  }) {
    return <Object?>[
      row,
      line.toString(),
      <Object?>[
        for (var column = 0; column < line.length; column++)
          renderCellMessage(
            line,
            column,
            bufferRow: bufferRow,
            hyperlinkUpdates: hyperlinkUpdates,
          ),
      ],
    ];
  }

  void refreshViewportCache() {
    final first = terminal.buffer.scrollBack;
    viewportCache = <_TerminalXtermWorkerRowCache>[
      for (var row = 0; row < terminal.viewHeight; row++)
        _TerminalXtermWorkerRowCache.capture(
          terminal.buffer.lines[first + row],
        ),
    ];
    cachedCols = terminal.viewWidth;
    cachedRows = terminal.viewHeight;
  }

  List<Object?> globalStateMessage() {
    return <Object?>[
      terminal.isUsingAltBuffer,
      terminal.reverseDisplayMode,
      terminal.applicationCursorType?.index,
      terminal.cursorBlinkMode,
      terminal.cursorLineHighlightMode,
      terminal.mouseShiftCaptureMode,
      terminal.altEscPrefixMode,
      terminal.altSendsEscapeMode,
      terminal.colorRevision,
      <Object?>[
        for (final entry in terminal.indexedColorOverrides)
          <Object?>[entry.key, entry.value],
      ],
      <Object?>[
        for (final entry in terminal.specialColorOverrides)
          <Object?>[entry.key, entry.value],
      ],
      terminal.foregroundColorOverride,
      terminal.backgroundColorOverride,
      terminal.cursorColorOverride,
      terminal.selectionColorOverride,
      terminal.selectionForegroundColorOverride,
    ];
  }

  List<Object?> snapshot() {
    final first = terminal.buffer.scrollBack;
    final viewportRows = <String>[
      for (var row = 0; row < terminal.viewHeight; row++)
        terminal.buffer.lines[first + row].toString(),
    ];
    refreshViewportCache();
    return <Object?>[
      viewportRows,
      terminal.buffer.cursorX,
      terminal.buffer.cursorY,
      terminal.cursorVisibleMode,
      terminal.cursorKeysMode,
      terminal.appKeypadMode,
      terminal.bracketedPasteMode,
      terminal.reportFocusMode,
      terminal.altBufferMouseScrollMode,
      terminal.mouseMode.index,
      terminal.mouseReportMode.index,
      terminal.buffer.scrollBack,
      <Object?>[for (final effect in effects) List<Object?>.from(effect)],
    ];
  }

  List<Object?> delta({bool forceFullRepaint = false}) {
    final first = terminal.buffer.scrollBack;
    final previous = viewportCache;
    final fullRepaint =
        forceFullRepaint ||
        previous == null ||
        cachedCols != terminal.viewWidth ||
        cachedRows != terminal.viewHeight;
    final nextCache = <_TerminalXtermWorkerRowCache>[];
    final changedRows = <Object?>[];

    for (var row = 0; row < terminal.viewHeight; row++) {
      final line = terminal.buffer.lines[first + row];
      final previousRow = !fullRepaint && row < previous.length
          ? previous[row]
          : null;
      if (previousRow != null && previousRow.matches(line)) {
        nextCache.add(previousRow);
        continue;
      }
      nextCache.add(_TerminalXtermWorkerRowCache.capture(line));
      changedRows.add(rowMessage(row, line));
    }

    viewportCache = nextCache;
    cachedCols = terminal.viewWidth;
    cachedRows = terminal.viewHeight;
    return <Object?>[
      fullRepaint,
      terminal.viewWidth,
      terminal.viewHeight,
      changedRows,
      terminal.buffer.cursorX,
      terminal.buffer.cursorY,
      terminal.cursorVisibleMode,
      terminal.cursorKeysMode,
      terminal.appKeypadMode,
      terminal.bracketedPasteMode,
      terminal.reportFocusMode,
      terminal.altBufferMouseScrollMode,
      terminal.mouseMode.index,
      terminal.mouseReportMode.index,
      terminal.buffer.scrollBack,
      <Object?>[for (final effect in effects) List<Object?>.from(effect)],
      revision,
    ];
  }

  List<Object?> bufferDelta() {
    final lines = terminal.buffer.lines;
    final previousRefs = bufferLineRefs;
    final previousCaches = bufferLineCaches;
    var fullRepaint = previousRefs == null || previousCaches == null;
    var trimStart = 0;
    var overlap = 0;

    if (!fullRepaint) {
      if (lines.length == 0 || previousRefs.isEmpty) {
        fullRepaint = lines.length != previousRefs.length;
      } else {
        var retainedStart = -1;
        final firstLine = lines[0];
        for (var index = 0; index < previousRefs.length; index++) {
          if (identical(previousRefs[index], firstLine)) {
            retainedStart = index;
            break;
          }
        }

        if (retainedStart < 0) {
          fullRepaint = true;
        } else {
          final availablePrevious = previousRefs.length - retainedStart;
          overlap = availablePrevious < lines.length
              ? availablePrevious
              : lines.length;
          if (retainedStart + overlap != previousRefs.length) {
            fullRepaint = true;
          } else {
            for (var row = 0; row < overlap; row++) {
              if (!identical(previousRefs[retainedStart + row], lines[row])) {
                fullRepaint = true;
                break;
              }
            }
          }
          if (!fullRepaint) {
            trimStart = retainedStart;
          }
        }
      }
    }

    if (fullRepaint) {
      trimStart = 0;
      overlap = 0;
    }

    final nextRefs = <BufferLine>[];
    final nextCaches = <_TerminalXtermWorkerRowCache>[];
    final changedRows = <Object?>[];
    final hyperlinkUpdates = <int, String>{};
    for (var row = 0; row < lines.length; row++) {
      final line = lines[row];
      nextRefs.add(line);
      final previousCache =
          !fullRepaint && previousCaches != null && row < overlap
          ? previousCaches[trimStart + row]
          : null;
      if (previousCache != null && previousCache.matches(line)) {
        nextCaches.add(previousCache);
        continue;
      }
      nextCaches.add(_TerminalXtermWorkerRowCache.capture(line));
      changedRows.add(
        rowMessage(
          row,
          line,
          bufferRow: row,
          hyperlinkUpdates: hyperlinkUpdates,
        ),
      );
    }

    bufferLineRefs = nextRefs;
    bufferLineCaches = nextCaches;
    return <Object?>[
      fullRepaint,
      terminal.viewWidth,
      terminal.viewHeight,
      lines.length,
      terminal.buffer.scrollBack,
      trimStart,
      changedRows,
      terminal.buffer.cursorX,
      terminal.buffer.cursorY,
      terminal.cursorVisibleMode,
      terminal.cursorKeysMode,
      terminal.appKeypadMode,
      terminal.bracketedPasteMode,
      terminal.reportFocusMode,
      terminal.altBufferMouseScrollMode,
      terminal.mouseMode.index,
      terminal.mouseReportMode.index,
      <Object?>[for (final effect in effects) List<Object?>.from(effect)],
      revision,
      globalStateMessage(),
      <Object?>[
        for (final entry in hyperlinkUpdates.entries)
          <Object?>[entry.key, entry.value],
      ],
    ];
  }

  List<Object?> actionResult(bool? handled) {
    return <Object?>[
      revision,
      handled,
      <Object?>[for (final effect in effects) List<Object?>.from(effect)],
    ];
  }

  void fail(SendPort reply, Object error, StackTrace stackTrace) {
    reply.send(<Object?>[_workerError, '$error\n$stackTrace']);
  }

  commands.listen((Object? raw) {
    if (raw is! List<Object?> || raw.length < 2 || raw[0] is! SendPort) {
      return;
    }
    final reply = raw[0]! as SendPort;
    final command = raw[1];
    try {
      switch (command) {
        case _workerWrite:
          effects.clear();
          terminal.write(raw[2]! as String);
          revision += 1;
          reply.send(snapshot());
        case _workerWriteDelta:
          effects.clear();
          terminal.write(raw[2]! as String);
          revision += 1;
          reply.send(delta());
        case _workerWriteBufferDelta:
          effects.clear();
          terminal.write(raw[2]! as String);
          revision += 1;
          reply.send(bufferDelta());
        case _workerResize:
          effects.clear();
          terminal.resize(raw[2]! as int, raw[3]! as int);
          revision += 1;
          reply.send(snapshot());
        case _workerResizeDelta:
          effects.clear();
          terminal.resize(raw[2]! as int, raw[3]! as int);
          revision += 1;
          reply.send(delta(forceFullRepaint: true));
        case _workerResizeBufferDelta:
          effects.clear();
          terminal.resize(raw[2]! as int, raw[3]! as int);
          revision += 1;
          bufferLineRefs = null;
          bufferLineCaches = null;
          reply.send(bufferDelta());
        case _workerKeyInput:
          effects.clear();
          final handled = terminal.keyInput(
            TerminalKey.values[raw[2]! as int],
            shift: raw[3]! as bool,
            alt: raw[4]! as bool,
            ctrl: raw[5]! as bool,
            superKey: raw[6]! as bool,
            capsLock: raw[7]! as bool,
            numLock: raw[8]! as bool,
            type: TerminalKeyEventType.values[raw[9]! as int],
            text: raw[10] as String?,
          );
          revision += 1;
          reply.send(actionResult(handled));
        case _workerTextInput:
          effects.clear();
          terminal.textInput(raw[2]! as String);
          revision += 1;
          reply.send(actionResult(null));
        case _workerPaste:
          effects.clear();
          terminal.paste(raw[2]! as String);
          revision += 1;
          reply.send(actionResult(null));
        case _workerFocusInput:
          effects.clear();
          terminal.focusInput(raw[2]! as bool);
          revision += 1;
          reply.send(actionResult(null));
        case _workerMouseInput:
          effects.clear();
          final pixelX = raw[10] as int?;
          final pixelY = raw[11] as int?;
          final handled = terminal.mouseInput(
            TerminalMouseButton.values[raw[2]! as int],
            TerminalMouseButtonState.values[raw[3]! as int],
            CellOffset(raw[4]! as int, raw[5]! as int),
            motion: raw[6]! as bool,
            modifiers: TerminalMouseModifiers(
              shift: raw[7]! as bool,
              alt: raw[8]! as bool,
              control: raw[9]! as bool,
            ),
            pixelPosition: pixelX == null || pixelY == null
                ? null
                : CellOffset(pixelX, pixelY),
          );
          revision += 1;
          reply.send(actionResult(handled));
        case _workerClose:
          reply.send(const <Object?>[true]);
          commands.close();
        default:
          throw ArgumentError.value(command, 'command');
      }
    } catch (error, stackTrace) {
      fail(reply, error, stackTrace);
    }
  });

  owner.send(<Object?>[_workerReady, commands.sendPort]);
}
