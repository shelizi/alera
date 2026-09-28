import 'dart:async';
import 'dart:isolate';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:xterm2/xterm.dart';

import '../domain/terminal_mode_reset.dart';
import '../domain/terminal_osc52_clipboard.dart';

const String _workerReady = 'ready';
const String _workerWrite = 'write';
const String _workerWriteDelta = 'writeDelta';
const String _workerWriteBufferDelta = 'writeBufferDelta';
const String _workerHydrateSnapshotBufferDelta = 'hydrateSnapshotBufferDelta';
const String _workerProfileHydrateSnapshotBufferDelta =
    'profileHydrateSnapshotBufferDelta';
const String _workerParseHidden = 'parseHidden';
const String _workerSnapshotBufferDelta = 'snapshotBufferDelta';
const String _workerCatchUpBufferDelta = 'catchUpBufferDelta';
const String _workerProfileSnapshotBufferDelta = 'profileSnapshotBufferDelta';
const String _workerProfilePackedSnapshotBufferDelta =
    'profilePackedSnapshotBufferDelta';
const String _workerResize = 'resize';
const String _packedBufferRowsTag = 'packedBufferRowsV1';
const int _packedCellStride = 8;
const String _workerResizeDelta = 'resizeDelta';
const String _workerResizeBufferDelta = 'resizeBufferDelta';
const String _workerKeyInput = 'keyInput';
const String _workerTextInput = 'textInput';
const String _workerPaste = 'paste';
const String _workerFocusInput = 'focusInput';
const String _workerMouseInput = 'mouseInput';
const String _workerExportRetainedState = 'exportRetainedState';
const String _workerClose = 'close';
const String _retainedStateTag = 'retainedTerminalStateV1';
const int _retainedBufferCellStride = 5;
const String _workerError = 'error';
const String _effectTitleChanged = 'titleChanged';
const String _effectBell = 'bell';
const String _effectPtyWrite = 'ptyWrite';
const String _effectClipboardStore = 'clipboardStore';

final class TerminalXtermWorkerRetainedState {
  const TerminalXtermWorkerRetainedState._(this._message);

  final List<Object?> _message;
}

final class TerminalXtermWorkerRetainedStateExport {
  const TerminalXtermWorkerRetainedStateExport({
    required this.state,
    required this.blockers,
  });

  factory TerminalXtermWorkerRetainedStateExport._fromMessage(
    List<Object?> message,
  ) {
    final rawState = message[0];
    return TerminalXtermWorkerRetainedStateExport(
      state: rawState == null
          ? null
          : TerminalXtermWorkerRetainedState._(
              List<Object?>.from(rawState as List),
            ),
      blockers: List<String>.from(message[1]! as List),
    );
  }

  final TerminalXtermWorkerRetainedState? state;
  final List<String> blockers;

  bool get eligible => state != null;
}

final class _RetainedParserGroundTracker {
  int _state = 0;
  bool _escapeInString = false;
  int? _pendingHighSurrogate;

  bool get isGround =>
      _state == 0 && !_escapeInString && _pendingHighSurrogate == null;

  void reset() {
    _state = 0;
    _escapeInString = false;
    _pendingHighSurrogate = null;
  }

  void accept(String input) {
    if (_pendingHighSurrogate != null && input.isNotEmpty) {
      _pendingHighSurrogate = null;
    }
    if (input.isNotEmpty) {
      final last = input.codeUnitAt(input.length - 1);
      if (last >= 0xd800 && last <= 0xdbff) {
        _pendingHighSurrogate = last;
      }
    }
    for (var index = 0; index < input.length; index++) {
      final code = input.codeUnitAt(index);
      if (code >= 0xd800 && code <= 0xdbff && index == input.length - 1) {
        continue;
      }
      if (code == 0x18 || code == 0x1a) {
        _state = 0;
        _escapeInString = false;
        continue;
      }
      switch (_state) {
        case 0:
          if (code == 0x1b) {
            _state = 1;
          } else if (code == 0x9b) {
            _state = 2;
          } else if (code == 0x9d) {
            _state = 3;
          } else if (code == 0x90 ||
              code == 0x98 ||
              code == 0x9e ||
              code == 0x9f) {
            _state = 4;
          }
        case 1:
          if (code == 0x1b) {
            continue;
          }
          if (code == 0x5b) {
            _state = 2;
          } else if (code == 0x5d) {
            _state = 3;
          } else if (code == 0x50 ||
              code == 0x58 ||
              code == 0x5e ||
              code == 0x5f) {
            _state = 4;
          } else if (code >= 0x30 && code <= 0x7e) {
            _state = 0;
          }
        case 2:
          if (code == 0x1b) {
            _state = 1;
          } else if (code >= 0x40 && code <= 0x7e) {
            _state = 0;
          }
        case 3:
        case 4:
          if (code == 0x9c || (_state == 3 && code == 0x07)) {
            _state = 0;
            _escapeInString = false;
          } else if (_escapeInString) {
            if (code == 0x5c) {
              _state = 0;
            }
            _escapeInString = false;
          } else if (code == 0x1b) {
            _escapeInString = true;
          }
      }
    }
  }
}

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
    this._text,
    required this.width,
    required this.foreground,
    required this.background,
    required this.attributes,
    required this.underlineColor,
    required this.content,
    required this.hyperlinkId,
    required this.semanticAttributes,
    required this.combiningCharacters,
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
      combiningCharacters: message[9] as String?,
    );
  }

  final String? _text;
  final int width;
  final int foreground;
  final int background;
  final int attributes;
  final int underlineColor;
  final int content;
  final int hyperlinkId;
  final int semanticAttributes;
  final String? combiningCharacters;

  String get text {
    final text = _text;
    if (text != null) return text;
    final codePoint = content & CellContent.codepointMask;
    if (codePoint == 0) return '';
    return String.fromCharCode(codePoint) + (combiningCharacters ?? '');
  }
}

final class TerminalXtermWorkerRowDelta {
  const TerminalXtermWorkerRowDelta({
    required this.row,
    required this.rowLength,
    required this.cellStart,
    required this.text,
    required this.cells,
    required this.isWrapped,
    required this.isSemanticPromptLine,
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
      isWrapped: message[3]! as bool,
      isSemanticPromptLine: message[4]! as bool,
      cellStart: message[5]! as int,
      rowLength: message[6]! as int,
    );
  }

  final int row;
  final int rowLength;
  final int cellStart;
  final String text;
  final List<TerminalXtermWorkerRenderCell> cells;
  final bool isWrapped;
  final bool isSemanticPromptLine;
}

List<TerminalXtermWorkerRowDelta> _decodePackedWorkerRows(Object? raw) {
  final message = List<Object?>.from(raw! as List);
  if (message.length != 4 || message[0] != _packedBufferRowsTag) {
    throw StateError('Unexpected packed terminal buffer row payload.');
  }
  final rowTexts = List<String>.from(message[1]! as List);
  final packed = message[2];
  if (packed is! TransferableTypedData) {
    throw StateError('Packed terminal buffer rows are missing cell data.');
  }
  final words = packed.materialize().asUint32List();
  final combining = message[3]! as List;
  final rows = <TerminalXtermWorkerRowDelta>[];
  var wordOffset = 0;
  var combiningOffset = 0;
  var absoluteCell = 0;

  for (var row = 0; row < rowTexts.length; row++) {
    if (wordOffset + 2 > words.length) {
      throw StateError(
        'Packed terminal row metadata is truncated at row $row.',
      );
    }
    final rowLength = words[wordOffset++];
    final flags = words[wordOffset++];
    final cells = List<TerminalXtermWorkerRenderCell>.generate(rowLength, (
      column,
    ) {
      if (wordOffset + _packedCellStride > words.length) {
        throw StateError(
          'Packed terminal cell data is truncated at row $row column $column.',
        );
      }
      String? combiningCharacters;
      if (combiningOffset < combining.length &&
          combining[combiningOffset] == absoluteCell) {
        combiningCharacters = combining[combiningOffset + 1]! as String;
        combiningOffset += 2;
      }
      final cell = TerminalXtermWorkerRenderCell(
        width: words[wordOffset++],
        foreground: words[wordOffset++],
        background: words[wordOffset++],
        attributes: words[wordOffset++],
        underlineColor: words[wordOffset++],
        content: words[wordOffset++],
        hyperlinkId: words[wordOffset++],
        semanticAttributes: words[wordOffset++],
        combiningCharacters: combiningCharacters,
      );
      absoluteCell += 1;
      return cell;
    }, growable: false);
    rows.add(
      TerminalXtermWorkerRowDelta(
        row: row,
        rowLength: rowLength,
        cellStart: 0,
        text: rowTexts[row],
        cells: cells,
        isWrapped: flags & 1 != 0,
        isSemanticPromptLine: flags & 2 != 0,
      ),
    );
  }

  if (wordOffset != words.length || combiningOffset != combining.length) {
    throw StateError('Packed terminal buffer row payload has trailing data.');
  }
  return rows;
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
    required this.lineFeedMode,
    required this.ignoreKeypadWithNumLockMode,
    required this.backarrowKeyMode,
    required this.kittyKeyboardMode,
    required this.modifyOtherKeysMode,
    required this.keyboardActionMode,
    required this.synchronizedUpdate,
    required this.synchronizedUpdateGeneration,
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
      lineFeedMode: message[16]! as bool,
      ignoreKeypadWithNumLockMode: message[17]! as bool,
      backarrowKeyMode: message[18]! as bool,
      kittyKeyboardMode: message[19]! as int,
      modifyOtherKeysMode: message[20]! as int,
      keyboardActionMode: message[21]! as bool,
      synchronizedUpdate: message[22]! as bool,
      synchronizedUpdateGeneration: message[23]! as int,
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
  final bool lineFeedMode;
  final bool ignoreKeypadWithNumLockMode;
  final bool backarrowKeyMode;
  final int kittyKeyboardMode;
  final int modifyOtherKeysMode;
  final bool keyboardActionMode;
  final bool synchronizedUpdate;
  final int synchronizedUpdateGeneration;
}

final class TerminalXtermWorkerStateDelta {
  const TerminalXtermWorkerStateDelta({
    required this.revision,
    required this.cols,
    required this.rows,
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
    required this.globalState,
  });

  factory TerminalXtermWorkerStateDelta._fromMessage(List<Object?> message) {
    return TerminalXtermWorkerStateDelta(
      revision: message[0]! as int,
      cols: message[1]! as int,
      rows: message[2]! as int,
      cursorX: message[3]! as int,
      cursorY: message[4]! as int,
      cursorVisible: message[5]! as bool,
      cursorKeys: message[6]! as bool,
      keypadKeys: message[7]! as bool,
      bracketedPaste: message[8]! as bool,
      focusEvents: message[9]! as bool,
      altScroll: message[10]! as bool,
      mouseMode: message[11]! as int,
      mouseReportMode: message[12]! as int,
      scrollBack: message[13]! as int,
      effects: _decodeWorkerEffects(message[14]),
      globalState: TerminalXtermWorkerGlobalState._fromMessage(
        List<Object?>.from(message[15]! as List),
      ),
    );
  }

  final int revision;
  final int cols;
  final int rows;
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
  final TerminalXtermWorkerGlobalState globalState;
}

final class _TerminalXtermWorkerTerminal extends Terminal {
  _TerminalXtermWorkerTerminal({
    required int maxLines,
    required TerminalTargetPlatform platform,
    Set<int>? wordSeparators,
    void Function(String)? onTitleChange,
    void Function()? onBell,
    void Function(String)? onOutput,
    void Function(String, String)? onClipboardStore,
  }) : super(
         maxLines: maxLines,
         reflowWithHiddenCursor: false,
         preserveOrphanCombiningMarks: true,
         allowITerm2ClipboardCapture: false,
         allowKittyClipboard: false,
         onClipboardQuery: (_) => null,
         clipboardDecoder: decodeTerminalOsc52Payload,
         platform: platform,
         wordSeparators: wordSeparators,
         onTitleChange: onTitleChange,
         onBell: onBell,
         onOutput: onOutput,
         onClipboardStore: onClipboardStore,
       );

  bool keyboardActionMode = false;
  bool synchronizedUpdateMode = false;
  int synchronizedUpdateGeneration = 0;
  bool sendReceiveMode = true;
  bool enableColumnMode = false;
  bool slowScrollMode = false;
  bool autoRepeatMode = false;
  bool leftRightMarginMode = false;
  bool focused = true;
  int protectionMode = 0;
  int precedingCodepoint = 0;
  String? currentTitle;
  String? currentIconTitle;
  List<Object?>? mainSavedCursor;
  List<Object?>? altSavedCursor;
  final Set<String> retainedStateBlockers = <String>{};
  final Set<int> auxiliaryDynamicColorCodes = <int>{};
  Timer? _synchronizedUpdateMirrorTimer;

  List<Object?> _styleMessage() => <Object?>[
    cursor.foreground,
    cursor.background,
    cursor.underlineColor,
    cursor.attrs,
    cursor.hyperlinkId,
    cursor.semanticAttrs,
  ];

  void _applyStyleMessage(List<Object?> message) {
    cursor
      ..foreground = message[0]! as int
      ..background = message[1]! as int
      ..underlineColor = message[2]! as int
      ..attrs = message[3]! as int
      ..hyperlinkId = message[4]! as int
      ..semanticAttrs = message[5]! as int;
  }

  List<Object?> _savedCursorMessage() => <Object?>[
    buffer.cursorX,
    buffer.cursorY,
    originMode,
    _styleMessage(),
  ];

  void _restoreSavedCursor(Buffer target, List<Object?>? state) {
    if (state == null) return;
    final currentX = target.cursorX;
    final currentY = target.cursorY;
    final currentStyle = _styleMessage();
    target.setCursor(state[0]! as int, state[1]! as int);
    _applyStyleMessage(List<Object?>.from(state[3]! as List));
    target.saveCursor(originMode: state[2]! as bool);
    target.setCursor(currentX, currentY);
    _applyStyleMessage(currentStyle);
  }

  List<Object?> _bufferMessage(Buffer target) {
    var wordCount = 0;
    for (var row = 0; row < target.lines.length; row++) {
      wordCount += 2 + target.lines[row].length * _retainedBufferCellStride;
    }
    final words = Uint32List(wordCount);
    final combiningEntries = <Object?>[];
    var wordOffset = 0;
    var absoluteCell = 0;
    for (var row = 0; row < target.lines.length; row++) {
      final line = target.lines[row];
      words[wordOffset++] = line.length;
      words[wordOffset++] = line.isWrapped ? 1 : 0;
      for (var column = 0; column < line.length; column++) {
        words[wordOffset++] = line.getForeground(column);
        words[wordOffset++] = line.getBackground(column);
        words[wordOffset++] = line.getAttributes(column);
        words[wordOffset++] = line.getUnderlineColor(column);
        words[wordOffset++] = line.getContent(column);
        final combining = line.getCombiningCharacters(column);
        if (combining != null) {
          combiningEntries
            ..add(absoluteCell)
            ..add(combining);
        }
        absoluteCell += 1;
      }
    }
    return <Object?>[
      target.cursorX,
      target.cursorY,
      target.marginTop,
      target.marginBottom,
      target.marginLeft,
      target.marginRight,
      words,
      combiningEntries,
    ];
  }

  void _restoreBuffer(Buffer target, List<Object?> message) {
    final words = message[6]! as Uint32List;
    final combiningEntries = message[7]! as List;
    target.lines.clear();
    var wordOffset = 0;
    var combiningOffset = 0;
    var absoluteCell = 0;
    while (wordOffset < words.length) {
      final length = words[wordOffset++];
      final isWrapped = words[wordOffset++] != 0;
      final line = BufferLine(length, isWrapped: isWrapped);
      for (var column = 0; column < length; column++) {
        final foreground = words[wordOffset++];
        final background = words[wordOffset++];
        final attributes = words[wordOffset++];
        final underlineColor = words[wordOffset++];
        final content = words[wordOffset++];
        line.setCellData(
          column,
          CellData(
            foreground: foreground,
            background: background,
            flags: attributes,
            underlineColor: underlineColor,
            content: content,
          ),
        );
        if (combiningOffset < combiningEntries.length &&
            combiningEntries[combiningOffset] == absoluteCell) {
          final combining = combiningEntries[combiningOffset + 1]! as String;
          for (final codePoint in combining.runes) {
            line.addCombiningCharacter(column, codePoint);
          }
          combiningOffset += 2;
        }
        absoluteCell += 1;
      }
      target.lines.push(line);
    }
    target.setVerticalMargins(message[2]! as int, message[3]! as int);
    target.setHorizontalMargins(message[4]! as int, message[5]! as int);
    target.setCursor(message[0]! as int, message[1]! as int);
  }

  List<String> retainedStateEligibility({required bool parserGround}) {
    final blockers = <String>{...retainedStateBlockers};
    if (!parserGround) blockers.add('parser-not-ground');
    if (synchronizedUpdateMode) blockers.add('synchronized-update-active');
    if (mainBuffer.cursorX >= viewWidth - 1 ||
        altBuffer.cursorX >= viewWidth - 1) {
      blockers.add('pending-wrap-ambiguous');
    }
    if (indexedColorOverrides.isNotEmpty ||
        specialColorOverrides.isNotEmpty ||
        auxiliaryDynamicColorCodes.isNotEmpty ||
        foregroundColorOverride != null ||
        backgroundColorOverride != null ||
        cursorColorOverride != null ||
        selectionColorOverride != null ||
        selectionForegroundColorOverride != null) {
      blockers.add('custom-colors');
    }
    final semanticState = semanticPromptState;
    if (semanticState.content != TerminalSemanticPromptContent.output ||
        semanticState.lastCommandExitCode != null ||
        semanticState.aid != null ||
        semanticState.promptKind != null ||
        semanticState.clickMode != null ||
        semanticState.redraw != null ||
        semanticState.specialKey != null ||
        semanticState.commandLine != null) {
      blockers.add('semantic-shell-state');
    }
    if (cursor.hyperlinkId != 0 || cursor.semanticAttrs != 0) {
      blockers.add('cursor-extended-state');
    }
    for (final target in <Buffer>[mainBuffer, altBuffer]) {
      for (var row = 0; row < target.lines.length; row++) {
        final line = target.lines[row];
        for (var column = 0; column < line.length; column++) {
          if (line.getHyperlinkId(column) != 0) {
            blockers.add('hyperlinks');
          }
          if (line.getSemanticContent(column) != 0) {
            blockers.add('semantic-shell-state');
          }
        }
      }
    }
    final result = blockers.toList()..sort();
    return result;
  }

  List<Object?> retainedStateMessage() => <Object?>[
    _retainedStateTag,
    viewWidth,
    viewHeight,
    isUsingAltBuffer,
    _bufferMessage(mainBuffer),
    _bufferMessage(altBuffer),
    _styleMessage(),
    <Object?>[
      insertMode,
      sendReceiveMode,
      keyboardActionMode,
      lineFeedMode,
      cursorKeysMode,
      reverseDisplayMode,
      originMode,
      enableColumnMode,
      slowScrollMode,
      autoWrapMode,
      autoRepeatMode,
      reverseWrapMode,
      reverseWrapExtendedMode,
      mouseMode.index,
      mouseReportMode.index,
      cursorBlinkMode,
      cursorVisibleMode,
      applicationCursorType?.index,
      appKeypadMode,
      ignoreKeypadWithNumLockMode,
      backarrowKeyMode,
      reportFocusMode,
      mouseShiftCaptureMode,
      altBufferMouseScrollMode,
      altEscPrefixMode,
      altSendsEscapeMode,
      bracketedPasteMode,
      inBandSizeReportMode,
      reportColorSchemeMode,
      graphemeClusterMode,
      leftRightMarginMode,
      kittyKeyboardMode,
      modifyOtherKeysMode,
      protectionMode,
      cursorLineHighlightMode,
    ],
    mainSavedCursor,
    altSavedCursor,
    precedingCodepoint,
    focused,
    currentTitle,
    currentIconTitle,
  ];

  void restoreRetainedState(List<Object?> message) {
    if (message.isEmpty || message[0] != _retainedStateTag) {
      throw StateError('Unsupported terminal retained-state payload.');
    }
    reset();
    final modes = List<Object?>.from(message[7]! as List);
    setInsertMode(modes[0]! as bool);
    setSendReceiveMode(modes[1]! as bool);
    setKeyboardActionMode(modes[2]! as bool);
    setLineFeedMode(modes[3]! as bool);
    setCursorKeysMode(modes[4]! as bool);
    setReverseDisplayMode(modes[5]! as bool);
    setOriginMode(modes[6]! as bool);
    setEnableColumnMode(modes[7]! as bool);
    setSlowScrollMode(modes[8]! as bool);
    setAutoWrapMode(modes[9]! as bool);
    setAutoRepeatMode(modes[10]! as bool);
    setReverseWrapMode(modes[11]! as bool);
    setReverseWrapExtendedMode(modes[12]! as bool);
    setMouseMode(MouseMode.values[modes[13]! as int]);
    setMouseReportMode(MouseReportMode.values[modes[14]! as int]);
    setCursorBlinkMode(modes[15]! as bool);
    setCursorVisibleMode(modes[16]! as bool);
    final cursorType = modes[17] as int?;
    setCursorShape(switch (cursorType) {
      null => 0,
      0 => 2,
      1 => 4,
      2 => 6,
      _ => 0,
    });
    setAppKeypadMode(modes[18]! as bool);
    setIgnoreKeypadWithNumLockMode(modes[19]! as bool);
    setBackarrowKeyMode(modes[20]! as bool);
    setMouseShiftCaptureMode(modes[22]! as bool);
    setMouseReportMode(MouseReportMode.values[modes[14]! as int]);
    setAltBufferMouseScrollMode(modes[23]! as bool);
    setAltEscPrefixMode(modes[24]! as bool);
    setAltSendsEscapeMode(modes[25]! as bool);
    setBracketedPasteMode(modes[26]! as bool);
    setGraphemeClusterMode(modes[29]! as bool);
    setLeftRightMarginMode(modes[30]! as bool);
    setKittyKeyboardMode(modes[31]! as int, 1);
    setModifyOtherKeysMode(4, modes[32]! as int);
    protectionMode = modes[33]! as int;
    if (protectionMode == 1) {
      setProtectedMode(true);
    } else if (protectionMode == 2) {
      setIsoProtectedMode(true);
    }
    setCursorLineHighlight(modes[34]! as bool);
    _restoreBuffer(mainBuffer, List<Object?>.from(message[4]! as List));
    _restoreBuffer(altBuffer, List<Object?>.from(message[5]! as List));
    if (message[3]! as bool) {
      useAltBuffer();
    } else {
      useMainBuffer();
    }
    _applyStyleMessage(List<Object?>.from(message[6]! as List));
    mainSavedCursor = message[8] == null
        ? null
        : List<Object?>.from(message[8]! as List);
    altSavedCursor = message[9] == null
        ? null
        : List<Object?>.from(message[9]! as List);
    _restoreSavedCursor(mainBuffer, mainSavedCursor);
    _restoreSavedCursor(altBuffer, altSavedCursor);
    precedingCodepoint = message[10]! as int;
    focused = message[11]! as bool;
    currentTitle = message[12] as String?;
    currentIconTitle = message[13] as String?;
    if (currentTitle != null) super.setTitle(currentTitle!);
    if (currentIconTitle != null) super.setIconName(currentIconTitle!);
    super.focusInput(focused);
    setReportFocusMode(modes[21]! as bool);
    setInBandSizeReportMode(modes[27]! as bool);
    setReportColorSchemeMode(modes[28]! as bool);
    retainedStateBlockers.clear();
  }

  @override
  void writeChar(int char) {
    super.writeChar(char);
    precedingCodepoint = char;
  }

  @override
  void writeText(String text, int start, int end) {
    super.writeText(text, start, end);
    if (start < end) precedingCodepoint = text.codeUnitAt(end - 1);
  }

  @override
  void repeatPreviousCharacter(int count) {
    if (precedingCodepoint == 0) return;
    for (var index = 0; index < count; index++) {
      buffer.writeChar(precedingCodepoint);
    }
  }

  @override
  void saveCursor() {
    final saved = _savedCursorMessage();
    if (isUsingAltBuffer) {
      altSavedCursor = saved;
    } else {
      mainSavedCursor = saved;
    }
    super.saveCursor();
  }

  @override
  void setSendReceiveMode(bool enabled) {
    sendReceiveMode = enabled;
    super.setSendReceiveMode(enabled);
  }

  @override
  void setKeyboardActionMode(bool enabled) {
    keyboardActionMode = enabled;
    super.setKeyboardActionMode(enabled);
  }

  @override
  void setEnableColumnMode(bool enabled) {
    enableColumnMode = enabled;
    super.setEnableColumnMode(enabled);
  }

  @override
  void setSlowScrollMode(bool enabled) {
    slowScrollMode = enabled;
    super.setSlowScrollMode(enabled);
  }

  @override
  void setAutoRepeatMode(bool enabled) {
    autoRepeatMode = enabled;
    super.setAutoRepeatMode(enabled);
  }

  @override
  void setLeftRightMarginMode(bool enabled) {
    leftRightMarginMode = enabled;
    super.setLeftRightMarginMode(enabled);
  }

  @override
  void setProtectedMode(bool enabled) {
    protectionMode = enabled ? 1 : 0;
    super.setProtectedMode(enabled);
  }

  @override
  void setIsoProtectedMode(bool enabled) {
    protectionMode = enabled ? 2 : 0;
    super.setIsoProtectedMode(enabled);
  }

  @override
  void focusInput(bool value) {
    focused = value;
    super.focusInput(value);
  }

  @override
  void setTitle(String name) {
    currentTitle = name;
    super.setTitle(name);
  }

  @override
  void setIconName(String name) {
    currentIconTitle = name;
    super.setIconName(name);
  }

  @override
  void setTapStop() {
    retainedStateBlockers.add('custom-tab-stops');
    super.setTapStop();
  }

  @override
  void clearTabStopUnderCursor() {
    retainedStateBlockers.add('custom-tab-stops');
    super.clearTabStopUnderCursor();
  }

  @override
  void clearAllTabStops() {
    retainedStateBlockers.add('custom-tab-stops');
    super.clearAllTabStops();
  }

  @override
  void designateCharset(int charset, int name) {
    retainedStateBlockers.add('custom-charset');
    super.designateCharset(charset, name);
  }

  @override
  void useCharset(int charset) {
    if (charset != 0) retainedStateBlockers.add('custom-charset');
    super.useCharset(charset);
  }

  @override
  void singleShiftCharset(int charset) {
    retainedStateBlockers.add('custom-charset');
    super.singleShiftCharset(charset);
  }

  @override
  void saveDecMode(int mode) {
    retainedStateBlockers.add('saved-dec-modes');
    super.saveDecMode(mode);
  }

  @override
  void pushKittyKeyboardMode(int mode) {
    retainedStateBlockers.add('kitty-keyboard-stack');
    super.pushKittyKeyboardMode(mode);
  }

  @override
  void popKittyKeyboardModes(int count) {
    retainedStateBlockers.add('kitty-keyboard-stack');
    super.popKittyKeyboardModes(count);
  }

  @override
  void pushTitle() {
    retainedStateBlockers.add('title-stack');
    super.pushTitle();
  }

  @override
  void popTitle() {
    retainedStateBlockers.add('title-stack');
    super.popTitle();
  }

  @override
  void setDynamicColor(int code, String value) {
    if (code == 13 || code == 14 || code == 15 || code == 16 || code == 18) {
      auxiliaryDynamicColorCodes.add(code);
    }
    super.setDynamicColor(code, value);
  }

  @override
  void resetDynamicColor(int code) {
    auxiliaryDynamicColorCodes.remove(code);
    super.resetDynamicColor(code);
  }

  @override
  void setHyperlink(String params, String uri) {
    if (uri.isNotEmpty) retainedStateBlockers.add('hyperlinks');
    super.setHyperlink(params, uri);
  }

  @override
  void setSynchronizedUpdateMode(bool enabled) {
    synchronizedUpdateGeneration += 1;
    synchronizedUpdateMode = enabled;
    _synchronizedUpdateMirrorTimer?.cancel();
    _synchronizedUpdateMirrorTimer = null;
    super.setSynchronizedUpdateMode(enabled);
    if (!enabled) return;
    _synchronizedUpdateMirrorTimer = Timer(
      const Duration(milliseconds: 150),
      () {
        synchronizedUpdateMode = false;
        _synchronizedUpdateMirrorTimer = null;
      },
    );
  }

  @override
  void reset() {
    _synchronizedUpdateMirrorTimer?.cancel();
    _synchronizedUpdateMirrorTimer = null;
    if (synchronizedUpdateMode) synchronizedUpdateGeneration += 1;
    synchronizedUpdateMode = false;
    keyboardActionMode = false;
    sendReceiveMode = true;
    enableColumnMode = false;
    slowScrollMode = false;
    autoRepeatMode = false;
    leftRightMarginMode = false;
    focused = true;
    protectionMode = 0;
    precedingCodepoint = 0;
    currentTitle = null;
    currentIconTitle = null;
    mainSavedCursor = null;
    altSavedCursor = null;
    auxiliaryDynamicColorCodes.clear();
    retainedStateBlockers.clear();
    super.reset();
  }

  @override
  void dispose() {
    _synchronizedUpdateMirrorTimer?.cancel();
    _synchronizedUpdateMirrorTimer = null;
    super.dispose();
  }
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
    required this.comparedRowCount,
    required this.cachedRowCount,
  });

  factory TerminalXtermWorkerBufferDelta._fromMessage(List<Object?> message) {
    final rawRows = message[6]! as List;
    final rowDeltas =
        rawRows.isNotEmpty && rawRows.first == _packedBufferRowsTag
        ? _decodePackedWorkerRows(rawRows)
        : <TerminalXtermWorkerRowDelta>[
            for (final raw in rawRows)
              TerminalXtermWorkerRowDelta._fromMessage(
                List<Object?>.from(raw as List),
              ),
          ];
    return TerminalXtermWorkerBufferDelta(
      fullRepaint: message[0]! as bool,
      cols: message[1]! as int,
      rows: message[2]! as int,
      bufferLength: message[3]! as int,
      scrollBack: message[4]! as int,
      trimStart: message[5]! as int,
      rowDeltas: rowDeltas,
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
      comparedRowCount: message[21]! as int,
      cachedRowCount: message[22]! as int,
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
  final int comparedRowCount;
  final int cachedRowCount;
}

final class TerminalXtermWorkerSnapshotHydrationProfile {
  const TerminalXtermWorkerSnapshotHydrationProfile({
    required this.delta,
    required this.parseMicros,
    required this.materializeMicros,
    required this.rawRoundtripMicros,
    required this.decodeMicros,
  });

  final TerminalXtermWorkerBufferDelta delta;
  final int parseMicros;
  final int materializeMicros;
  final int rawRoundtripMicros;
  final int decodeMicros;

  int get transferAndSchedulingMicros =>
      rawRoundtripMicros - parseMicros - materializeMicros;
}

final class TerminalXtermWorkerBufferDeltaProfile {
  const TerminalXtermWorkerBufferDeltaProfile({
    required this.delta,
    required this.materializeMicros,
    required this.rawRoundtripMicros,
    required this.decodeMicros,
  });

  final TerminalXtermWorkerBufferDelta delta;
  final int materializeMicros;
  final int rawRoundtripMicros;
  final int decodeMicros;

  int get transferAndSchedulingMicros => rawRoundtripMicros - materializeMicros;
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
    TerminalXtermWorkerRetainedState? retainedState,
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
        retainedState?._message,
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

  Future<TerminalXtermWorkerBufferDelta> writeBufferDelta(
    String data, {
    bool? focused,
  }) async {
    return TerminalXtermWorkerBufferDelta._fromMessage(
      await _requestRaw(<Object?>[_workerWriteBufferDelta, data, focused]),
    );
  }

  Future<TerminalXtermWorkerBufferDelta> hydrateSnapshotBufferDelta(
    String snapshot, {
    bool resetInteractionModes = false,
  }) async {
    return TerminalXtermWorkerBufferDelta._fromMessage(
      await _requestRaw(<Object?>[
        _workerHydrateSnapshotBufferDelta,
        snapshot,
        resetInteractionModes,
      ]),
    );
  }

  Future<TerminalXtermWorkerSnapshotHydrationProfile>
  profileHydrateSnapshotBufferDelta(
    String snapshot, {
    bool resetInteractionModes = false,
  }) async {
    final roundtripWatch = Stopwatch()..start();
    final raw = await _requestRaw(<Object?>[
      _workerProfileHydrateSnapshotBufferDelta,
      snapshot,
      resetInteractionModes,
    ]);
    roundtripWatch.stop();
    final decodeWatch = Stopwatch()..start();
    final delta = TerminalXtermWorkerBufferDelta._fromMessage(
      List<Object?>.from(raw[2]! as List),
    );
    decodeWatch.stop();
    return TerminalXtermWorkerSnapshotHydrationProfile(
      delta: delta,
      parseMicros: raw[0]! as int,
      materializeMicros: raw[1]! as int,
      rawRoundtripMicros: roundtripWatch.elapsedMicroseconds,
      decodeMicros: decodeWatch.elapsedMicroseconds,
    );
  }

  Future<TerminalXtermWorkerStateDelta> parseHidden(
    String data, {
    bool? focused,
  }) async {
    return TerminalXtermWorkerStateDelta._fromMessage(
      await _requestRaw(<Object?>[_workerParseHidden, data, focused]),
    );
  }

  Future<TerminalXtermWorkerRetainedStateExport> exportRetainedState() async {
    return TerminalXtermWorkerRetainedStateExport._fromMessage(
      await _requestRaw(const <Object?>[_workerExportRetainedState]),
    );
  }

  /// Brings a replica that already mirrors this worker's last buffer delta up
  /// to date, sending only rows that changed since then. Falls back to a full
  /// repaint when the worker has nothing to compare against.
  Future<TerminalXtermWorkerBufferDelta> catchUpBufferDelta() async {
    return TerminalXtermWorkerBufferDelta._fromMessage(
      await _requestRaw(const <Object?>[_workerCatchUpBufferDelta]),
    );
  }

  Future<TerminalXtermWorkerBufferDelta> snapshotBufferDelta() async {
    return TerminalXtermWorkerBufferDelta._fromMessage(
      await _requestRaw(const <Object?>[_workerSnapshotBufferDelta]),
    );
  }

  Future<TerminalXtermWorkerBufferDeltaProfile>
  profileSnapshotBufferDelta() async {
    final roundtripWatch = Stopwatch()..start();
    final raw = await _requestRaw(const <Object?>[
      _workerProfileSnapshotBufferDelta,
    ]);
    roundtripWatch.stop();
    final materializeMicros = raw[0]! as int;
    final decodeWatch = Stopwatch()..start();
    final delta = TerminalXtermWorkerBufferDelta._fromMessage(
      List<Object?>.from(raw[1]! as List),
    );
    decodeWatch.stop();
    return TerminalXtermWorkerBufferDeltaProfile(
      delta: delta,
      materializeMicros: materializeMicros,
      rawRoundtripMicros: roundtripWatch.elapsedMicroseconds,
      decodeMicros: decodeWatch.elapsedMicroseconds,
    );
  }

  Future<TerminalXtermWorkerBufferDeltaProfile>
  profilePackedSnapshotBufferDelta() async {
    final roundtripWatch = Stopwatch()..start();
    final raw = await _requestRaw(const <Object?>[
      _workerProfilePackedSnapshotBufferDelta,
    ]);
    roundtripWatch.stop();
    final materializeMicros = raw[0]! as int;
    final decodeWatch = Stopwatch()..start();
    final delta = TerminalXtermWorkerBufferDelta._fromMessage(
      List<Object?>.from(raw[1]! as List),
    );
    decodeWatch.stop();
    return TerminalXtermWorkerBufferDeltaProfile(
      delta: delta,
      materializeMicros: materializeMicros,
      rawRoundtripMicros: roundtripWatch.elapsedMicroseconds,
      decodeMicros: decodeWatch.elapsedMicroseconds,
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
    int? pixelWidth,
    int? pixelHeight,
  }) async {
    return TerminalXtermWorkerBufferDelta._fromMessage(
      await _requestRaw(<Object?>[
        _workerResizeBufferDelta,
        cols,
        rows,
        pixelWidth,
        pixelHeight,
      ]),
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

  bool _cellMatches(BufferLine line, int column) {
    final offset = column * 5;
    if (offset + 4 >= cells.length ||
        cells[offset] != line.getForeground(column) ||
        cells[offset + 1] != line.getBackground(column) ||
        cells[offset + 2] != line.getAttributes(column) ||
        cells[offset + 3] != line.getContent(column) ||
        cells[offset + 4] != line.getUnderlineColor(column)) {
      return false;
    }
    return combiningCharacters?[column] == line.getCombiningCharacters(column);
  }

  _TerminalXtermWorkerCellSpan? changedCellSpan(BufferLine line) {
    if (cells.length != line.length * 5) {
      return _TerminalXtermWorkerCellSpan(0, line.length);
    }
    int? start;
    var end = 0;
    for (var column = 0; column < line.length; column++) {
      if (_cellMatches(line, column)) continue;
      start ??= column;
      end = column + 1;
    }
    return start == null ? null : _TerminalXtermWorkerCellSpan(start, end);
  }

  bool matches(BufferLine line) {
    return isWrapped == line.isWrapped && changedCellSpan(line) == null;
  }
}

final class _TerminalXtermWorkerCellSpan {
  const _TerminalXtermWorkerCellSpan(this.start, this.end);

  final int start;
  final int end;
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
  final rawRetainedState = initialization.length > 6
      ? initialization[6] as List?
      : null;
  final commands = ReceivePort();
  final effects = <List<Object?>>[];
  final parserGroundTracker = _RetainedParserGroundTracker();
  final terminal = _TerminalXtermWorkerTerminal(
    maxLines: maxLines,
    platform: platform,
    wordSeparators: wordSeparators,
    onTitleChange: (title) =>
        effects.add(<Object?>[_effectTitleChanged, title]),
    onBell: () => effects.add(const <Object?>[_effectBell]),
    onOutput: (value) => effects.add(<Object?>[_effectPtyWrite, value]),
    onClipboardStore: (selector, text) =>
        effects.add(<Object?>[_effectClipboardStore, selector, text]),
  )..resize(cols, rows);
  if (rawRetainedState != null) {
    terminal.restoreRetainedState(List<Object?>.from(rawRetainedState));
    effects.clear();
  }

  void writeTerminal(String data) {
    parserGroundTracker.accept(data);
    terminal.write(data);
  }

  var revision = 0;
  List<_TerminalXtermWorkerRowCache>? viewportCache;
  var cachedCols = 0;
  var cachedRows = 0;
  List<BufferLine>? bufferLineRefs;
  List<_TerminalXtermWorkerRowCache?>? bufferLineCaches;
  var cachedBufferScrollBack = 0;

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
      combining,
    ];
  }

  List<Object?> rowMessage(
    int row,
    BufferLine line, {
    int? bufferRow,
    Map<int, String>? hyperlinkUpdates,
    int cellStart = 0,
    int? cellEnd,
  }) {
    final end = cellEnd ?? line.length;
    return <Object?>[
      row,
      line.toString(),
      <Object?>[
        for (var column = cellStart; column < end; column++)
          renderCellMessage(
            line,
            column,
            bufferRow: bufferRow,
            hyperlinkUpdates: hyperlinkUpdates,
          ),
      ],
      line.isWrapped,
      bufferRow != null && terminal.isSemanticPromptLine(bufferRow),
      cellStart,
      line.length,
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
      terminal.lineFeedMode,
      terminal.ignoreKeypadWithNumLockMode,
      terminal.backarrowKeyMode,
      terminal.kittyKeyboardMode,
      terminal.modifyOtherKeysMode,
      terminal.keyboardActionMode,
      terminal.synchronizedUpdateMode,
      terminal.synchronizedUpdateGeneration,
    ];
  }

  List<Object?> stateDelta() {
    return <Object?>[
      revision,
      terminal.viewWidth,
      terminal.viewHeight,
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
      globalStateMessage(),
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

  List<Object?> packedFullBufferDelta() {
    final lines = terminal.buffer.lines;
    var totalCells = 0;
    for (var row = 0; row < lines.length; row++) {
      totalCells += lines[row].length;
    }
    final packedWords = Uint32List(
      lines.length * 2 + totalCells * _packedCellStride,
    );
    final rowTexts = List<String>.filled(lines.length, '', growable: false);
    final combiningEntries = <Object?>[];
    final hyperlinkUpdates = <int, String>{};
    final nextRefs = <BufferLine>[];
    final nextCaches = <_TerminalXtermWorkerRowCache?>[];
    final currentScrollBack = terminal.buffer.scrollBack;
    var wordOffset = 0;
    var absoluteCell = 0;

    for (var row = 0; row < lines.length; row++) {
      final line = lines[row];
      nextRefs.add(line);
      nextCaches.add(
        row < currentScrollBack
            ? null
            : _TerminalXtermWorkerRowCache.capture(line),
      );
      rowTexts[row] = line.toString();
      packedWords[wordOffset++] = line.length;
      packedWords[wordOffset++] =
          (line.isWrapped ? 1 : 0) |
          (terminal.isSemanticPromptLine(row) ? 2 : 0);

      for (var column = 0; column < line.length; column++) {
        final combining = line.getCombiningCharacters(column);
        if (combining != null) {
          combiningEntries
            ..add(absoluteCell)
            ..add(combining);
        }
        final hyperlinkId = line.getHyperlinkId(column);
        if (hyperlinkId != 0 && !hyperlinkUpdates.containsKey(hyperlinkId)) {
          final uri = terminal.hyperlinkAt(CellOffset(column, row));
          if (uri != null) {
            hyperlinkUpdates[hyperlinkId] = uri;
          }
        }
        packedWords[wordOffset++] = line.getWidth(column).toUnsigned(32);
        packedWords[wordOffset++] = line.getForeground(column).toUnsigned(32);
        packedWords[wordOffset++] = line.getBackground(column).toUnsigned(32);
        packedWords[wordOffset++] = line.getAttributes(column).toUnsigned(32);
        packedWords[wordOffset++] = line
            .getUnderlineColor(column)
            .toUnsigned(32);
        packedWords[wordOffset++] = line.getContent(column).toUnsigned(32);
        packedWords[wordOffset++] = hyperlinkId.toUnsigned(32);
        packedWords[wordOffset++] = line
            .getSemanticContent(column)
            .toUnsigned(32);
        absoluteCell += 1;
      }
    }

    if (wordOffset != packedWords.length) {
      throw StateError(
        'Packed terminal buffer wrote $wordOffset words; expected '
        '${packedWords.length}.',
      );
    }
    bufferLineRefs = nextRefs;
    bufferLineCaches = nextCaches;
    cachedBufferScrollBack = currentScrollBack;
    final cachedRowCount = nextCaches
        .whereType<_TerminalXtermWorkerRowCache>()
        .length;
    final packedRows = <Object?>[
      _packedBufferRowsTag,
      rowTexts,
      TransferableTypedData.fromList(<TypedData>[packedWords]),
      combiningEntries,
    ];
    return <Object?>[
      true,
      terminal.viewWidth,
      terminal.viewHeight,
      lines.length,
      currentScrollBack,
      0,
      packedRows,
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
      0,
      cachedRowCount,
    ];
  }

  List<Object?> bufferDelta({int? maxRevisitedRows}) {
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
            // xterm only replaces lines inside the scroll margins, which sit
            // in the viewport; scrollback lines leave only from the head. So
            // rows that were already scrollback keep their identity and the
            // check starts at the last of them, which keeps a keystroke echo
            // from walking the whole scrollback.
            final firstViewportRow = cachedBufferScrollBack - retainedStart;
            for (
              var row = firstViewportRow > 0 ? firstViewportRow - 1 : 0;
              row < overlap;
              row++
            ) {
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
      // Every row goes out either way, and the packed form costs one typed
      // buffer instead of a message list per cell on both isolates.
      return packedFullBufferDelta();
    }

    final changedRows = <Object?>[];
    final hyperlinkUpdates = <int, String>{};
    var comparedRowCount = 0;
    final currentScrollBack = terminal.buffer.scrollBack;
    // Rows that were scrollback and still are carry no cache and cannot have
    // changed, so the retained prefix of the previous lists is reused as is
    // and only the rows after it are revisited.
    var retainedRows = 0;
    if (!fullRepaint) {
      final previousScrollBackRows = cachedBufferScrollBack - trimStart;
      retainedRows = previousScrollBackRows < currentScrollBack
          ? previousScrollBackRows
          : currentScrollBack;
      if (retainedRows < 0) {
        retainedRows = 0;
      }
    }
    if (maxRevisitedRows != null &&
        lines.length - retainedRows > maxRevisitedRows) {
      return packedFullBufferDelta();
    }
    final nextRefs = <BufferLine>[];
    final nextCaches = <_TerminalXtermWorkerRowCache?>[];
    var cachedRowCount = 0;
    for (var row = retainedRows; row < lines.length; row++) {
      final line = lines[row];
      nextRefs.add(line);
      final previousCache =
          !fullRepaint && previousCaches != null && row < overlap
          ? previousCaches[trimStart + row]
          : null;
      final previousRow = trimStart + row;
      final wasAlreadyScrollback =
          !fullRepaint && previousRow < cachedBufferScrollBack;
      final remainsScrollback = row < currentScrollBack;
      if (wasAlreadyScrollback && remainsScrollback) {
        nextCaches.add(null);
        continue;
      }
      final changedSpan = switch (previousCache) {
        null => null,
        final cache => () {
          comparedRowCount += 1;
          return cache.changedCellSpan(line);
        }(),
      };
      if (previousCache != null &&
          previousCache.isWrapped == line.isWrapped &&
          changedSpan == null) {
        nextCaches.add(remainsScrollback ? null : previousCache);
        if (!remainsScrollback) cachedRowCount += 1;
        continue;
      }
      nextCaches.add(
        remainsScrollback ? null : _TerminalXtermWorkerRowCache.capture(line),
      );
      if (!remainsScrollback) cachedRowCount += 1;
      final cellStart = changedSpan?.start ?? 0;
      final cellEnd =
          changedSpan?.end ?? (previousCache == null ? line.length : 0);
      changedRows.add(
        rowMessage(
          row,
          line,
          bufferRow: row,
          hyperlinkUpdates: hyperlinkUpdates,
          cellStart: cellStart,
          cellEnd: cellEnd,
        ),
      );
    }

    if (retainedRows > 0) {
      // Shift the retained prefix into place and append the revisited rows,
      // instead of rebuilding lists as long as the whole buffer.
      previousRefs!
        ..removeRange(0, trimStart)
        ..length = retainedRows
        ..addAll(nextRefs);
      previousCaches!
        ..removeRange(0, trimStart)
        ..length = retainedRows
        ..addAll(nextCaches);
      bufferLineRefs = previousRefs;
      bufferLineCaches = previousCaches;
    } else {
      bufferLineRefs = nextRefs;
      bufferLineCaches = nextCaches;
    }
    cachedBufferScrollBack = currentScrollBack;
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
      comparedRowCount,
      cachedRowCount,
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
          writeTerminal(raw[2]! as String);
          revision += 1;
          reply.send(snapshot());
        case _workerWriteDelta:
          effects.clear();
          writeTerminal(raw[2]! as String);
          revision += 1;
          reply.send(delta());
        case _workerWriteBufferDelta:
          effects.clear();
          if (raw.length > 3) {
            final focused = raw[3];
            if (focused is bool) {
              terminal.focusInput(focused);
              effects.clear();
            }
          }
          writeTerminal(raw[2]! as String);
          revision += 1;
          reply.send(bufferDelta());
        case _workerHydrateSnapshotBufferDelta:
          effects.clear();
          writeTerminal(raw[2]! as String);
          if (raw.length > 3 && raw[3] == true) {
            writeTerminal(terminalInteractionModeReset);
          }
          revision += 1;
          bufferLineRefs = null;
          bufferLineCaches = null;
          reply.send(packedFullBufferDelta());
        case _workerProfileHydrateSnapshotBufferDelta:
          effects.clear();
          final parseWatch = Stopwatch()..start();
          writeTerminal(raw[2]! as String);
          if (raw.length > 3 && raw[3] == true) {
            writeTerminal(terminalInteractionModeReset);
          }
          parseWatch.stop();
          revision += 1;
          bufferLineRefs = null;
          bufferLineCaches = null;
          final materializeWatch = Stopwatch()..start();
          final message = packedFullBufferDelta();
          materializeWatch.stop();
          reply.send(<Object?>[
            parseWatch.elapsedMicroseconds,
            materializeWatch.elapsedMicroseconds,
            message,
          ]);
        case _workerParseHidden:
          effects.clear();
          if (raw.length > 3) {
            final focused = raw[3];
            if (focused is bool) {
              terminal.focusInput(focused);
              effects.clear();
            }
          }
          writeTerminal(raw[2]! as String);
          revision += 1;
          reply.send(stateDelta());
        case _workerExportRetainedState:
          effects.clear();
          final blockers = terminal.retainedStateEligibility(
            parserGround: parserGroundTracker.isGround,
          );
          reply.send(<Object?>[
            blockers.isEmpty ? terminal.retainedStateMessage() : null,
            blockers,
          ]);
        case _workerSnapshotBufferDelta:
          effects.clear();
          revision += 1;
          bufferLineRefs = null;
          bufferLineCaches = null;
          reply.send(packedFullBufferDelta());
        case _workerCatchUpBufferDelta:
          effects.clear();
          revision += 1;
          // A row sent on its own costs roughly 30 times its share of a packed
          // full buffer, so a long hidden backlog is cheaper to resend whole.
          final lineCount = terminal.buffer.lines.length;
          reply.send(
            bufferLineRefs == null
                ? packedFullBufferDelta()
                : bufferDelta(
                    maxRevisitedRows: math.max(
                      lineCount ~/ 32,
                      terminal.viewHeight * 2,
                    ),
                  ),
          );
        case _workerProfileSnapshotBufferDelta:
          effects.clear();
          revision += 1;
          bufferLineRefs = null;
          bufferLineCaches = null;
          final watch = Stopwatch()..start();
          final message = bufferDelta();
          watch.stop();
          reply.send(<Object?>[watch.elapsedMicroseconds, message]);
        case _workerProfilePackedSnapshotBufferDelta:
          effects.clear();
          revision += 1;
          bufferLineRefs = null;
          bufferLineCaches = null;
          final watch = Stopwatch()..start();
          final message = packedFullBufferDelta();
          watch.stop();
          reply.send(<Object?>[watch.elapsedMicroseconds, message]);
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
          terminal.resize(
            raw[2]! as int,
            raw[3]! as int,
            raw.length > 4 ? raw[4] as int? : null,
            raw.length > 5 ? raw[5] as int? : null,
          );
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
