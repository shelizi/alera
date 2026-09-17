/// Experimental VT parser worker boundary.
///
/// The Ghostty terminal, formatter, and render state stay inside the worker
/// isolate. The UI isolate only receives plain Dart values, so no native FFI
/// handle or Flutter rendering object crosses the isolate boundary.
library;

import 'dart:async';
import 'dart:isolate';
import 'dart:typed_data';

import 'package:ghostty_vte/ghostty_vte.dart';
import 'package:ghostty_vte/ghostty_vte_bindings_generated.dart'
    as ghostty_bindings;

const String _workerReady = 'ready';
const String _workerError = 'error';
const String _workerWrite = 'write';
const String _workerWriteDelta = 'writeDelta';
const String _workerResize = 'resize';
const String _workerClose = 'close';

const String _effectPtyWrite = 'ptyWrite';
const String _effectTitleChanged = 'titleChanged';
const String _effectBell = 'bell';

/// Side effect emitted synchronously while parsing terminal bytes.
sealed class TerminalVtWorkerEffect {
  const TerminalVtWorkerEffect();

  static TerminalVtWorkerEffect fromMessage(List<Object?> message) {
    return switch (message[0]) {
      _effectPtyWrite => TerminalVtWorkerPtyWrite(
        Uint8List.fromList(message[1]! as Uint8List),
      ),
      _effectTitleChanged => TerminalVtWorkerTitleChanged(
        message[1]! as String,
      ),
      _effectBell => const TerminalVtWorkerBell(),
      _ => throw StateError('Unknown terminal VT worker effect: ${message[0]}'),
    };
  }
}

/// Bytes that must be written back to the PTY, such as a DSR response.
final class TerminalVtWorkerPtyWrite extends TerminalVtWorkerEffect {
  const TerminalVtWorkerPtyWrite(this.bytes);

  final Uint8List bytes;
}

/// Terminal title update emitted by OSC 0/2.
final class TerminalVtWorkerTitleChanged extends TerminalVtWorkerEffect {
  const TerminalVtWorkerTitleChanged(this.title);

  final String title;
}

/// Audible/visual bell request emitted by BEL.
final class TerminalVtWorkerBell extends TerminalVtWorkerEffect {
  const TerminalVtWorkerBell();
}

List<TerminalVtWorkerEffect> _effectsFromMessage(Object? value) {
  if (value is! List) {
    return const <TerminalVtWorkerEffect>[];
  }
  return List<TerminalVtWorkerEffect>.unmodifiable(
    value.map(
      (effect) =>
          TerminalVtWorkerEffect.fromMessage((effect! as List).cast<Object?>()),
    ),
  );
}

/// Interaction modes that affect keyboard, paste, and pointer routing.
final class TerminalVtWorkerModes {
  const TerminalVtWorkerModes({
    required this.bracketedPaste,
    required this.cursorKeys,
    required this.keypadKeys,
    required this.cursorVisible,
    required this.mouseEnabled,
    required this.mouseTrackingMode,
    required this.mouseFormat,
    required this.focusEvents,
    required this.altScroll,
  });

  final bool bracketedPaste;
  final bool cursorKeys;
  final bool keypadKeys;
  final bool cursorVisible;
  final bool mouseEnabled;
  final int? mouseTrackingMode;
  final int? mouseFormat;
  final bool focusEvents;
  final bool altScroll;

  factory TerminalVtWorkerModes._fromMessage(List<Object?> message) {
    return TerminalVtWorkerModes(
      bracketedPaste: message[0]! as bool,
      cursorKeys: message[1]! as bool,
      keypadKeys: message[2]! as bool,
      cursorVisible: message[3]! as bool,
      mouseEnabled: message[4]! as bool,
      mouseTrackingMode: message[5] as int?,
      mouseFormat: message[6] as int?,
      focusEvents: message[7]! as bool,
      altScroll: message[8]! as bool,
    );
  }
}

/// Primitive Ghostty style-color token safe to send across isolates.
final class TerminalVtWorkerStyleColor {
  const TerminalVtWorkerStyleColor({
    required this.tag,
    required this.paletteIndex,
    required this.rgb,
  });

  final int tag;
  final int? paletteIndex;

  /// Packed 0xRRGGBB value when [tag] identifies an RGB color.
  final int? rgb;

  factory TerminalVtWorkerStyleColor._fromMessage(List<Object?> message) {
    return TerminalVtWorkerStyleColor(
      tag: message[0]! as int,
      paletteIndex: message[1] as int?,
      rgb: message[2] as int?,
    );
  }
}

/// Primitive cell style referenced by Ghostty style id inside one delta.
final class TerminalVtWorkerCellStyle {
  const TerminalVtWorkerCellStyle({
    required this.foreground,
    required this.background,
    required this.underlineColor,
    required this.bold,
    required this.italic,
    required this.faint,
    required this.blink,
    required this.inverse,
    required this.invisible,
    required this.strikethrough,
    required this.overline,
    required this.underline,
  });

  final TerminalVtWorkerStyleColor foreground;
  final TerminalVtWorkerStyleColor background;
  final TerminalVtWorkerStyleColor underlineColor;
  final bool bold;
  final bool italic;
  final bool faint;
  final bool blink;
  final bool inverse;
  final bool invisible;
  final bool strikethrough;
  final bool overline;
  final int underline;

  factory TerminalVtWorkerCellStyle._fromMessage(List<Object?> message) {
    return TerminalVtWorkerCellStyle(
      foreground: TerminalVtWorkerStyleColor._fromMessage(
        (message[0]! as List).cast<Object?>(),
      ),
      background: TerminalVtWorkerStyleColor._fromMessage(
        (message[1]! as List).cast<Object?>(),
      ),
      underlineColor: TerminalVtWorkerStyleColor._fromMessage(
        (message[2]! as List).cast<Object?>(),
      ),
      bold: message[3]! as bool,
      italic: message[4]! as bool,
      faint: message[5]! as bool,
      blink: message[6]! as bool,
      inverse: message[7]! as bool,
      invisible: message[8]! as bool,
      strikethrough: message[9]! as bool,
      overline: message[10]! as bool,
      underline: message[11]! as int,
    );
  }
}

/// Styled cell in one dirty viewport row.
final class TerminalVtWorkerRenderCell {
  const TerminalVtWorkerRenderCell({
    required this.text,
    required this.width,
    required this.wide,
    required this.hasText,
    required this.hasStyling,
    required this.styleId,
    required this.style,
    required this.hasHyperlink,
    required this.isProtected,
    required this.semanticContent,
    required this.backgroundPaletteIndex,
    required this.backgroundRgb,
  });

  final String text;
  final int width;

  /// Raw Ghostty wide-cell code: narrow=0, wide=1, tail=2, wrap head=3.
  final int wide;
  final bool hasText;
  final bool hasStyling;
  final int styleId;
  final TerminalVtWorkerCellStyle style;
  final bool hasHyperlink;
  final bool isProtected;
  final int semanticContent;
  final int? backgroundPaletteIndex;
  final int? backgroundRgb;

  factory TerminalVtWorkerRenderCell._fromMessage(
    List<Object?> message,
    Map<int, TerminalVtWorkerCellStyle> styles,
  ) {
    final styleId = message[5]! as int;
    final style = styles[styleId];
    if (style == null) {
      throw StateError('Missing terminal VT worker style $styleId.');
    }
    return TerminalVtWorkerRenderCell(
      text: message[0]! as String,
      width: message[1]! as int,
      wide: message[2]! as int,
      hasText: message[3]! as bool,
      hasStyling: message[4]! as bool,
      styleId: styleId,
      style: style,
      hasHyperlink: message[6]! as bool,
      isProtected: message[7]! as bool,
      semanticContent: message[8]! as int,
      backgroundPaletteIndex: message[9] as int?,
      backgroundRgb: message[10] as int?,
    );
  }
}

/// Pure-Dart state returned from the VT worker.
final class TerminalVtWorkerSnapshot {
  const TerminalVtWorkerSnapshot({
    required this.revision,
    required this.cols,
    required this.rows,
    required this.text,
    required this.viewportRows,
    required this.cursorVisible,
    required this.cursorX,
    required this.cursorY,
    required this.modes,
    this.effects = const <TerminalVtWorkerEffect>[],
  });

  final int revision;
  final int cols;
  final int rows;
  final String text;
  final List<String> viewportRows;
  final bool cursorVisible;
  final int? cursorX;
  final int? cursorY;
  final TerminalVtWorkerModes modes;
  final List<TerminalVtWorkerEffect> effects;

  factory TerminalVtWorkerSnapshot._fromMessage(List<Object?> message) {
    if (message.length < 9 || message[0] != true) {
      throw StateError('Invalid terminal VT worker snapshot.');
    }
    return TerminalVtWorkerSnapshot(
      revision: message[1]! as int,
      cols: message[2]! as int,
      rows: message[3]! as int,
      text: message[4]! as String,
      viewportRows: List<String>.unmodifiable(
        (message[5]! as List).cast<String>(),
      ),
      cursorVisible: message[6]! as bool,
      cursorX: message[7] as int?,
      cursorY: message[8] as int?,
      effects: _effectsFromMessage(message.length > 9 ? message[9] : null),
      modes: TerminalVtWorkerModes._fromMessage(
        (message[10]! as List).cast<Object?>(),
      ),
    );
  }
}

/// One changed viewport row returned by the VT worker.
final class TerminalVtWorkerRowDelta {
  const TerminalVtWorkerRowDelta({
    required this.row,
    required this.cells,
    required this.renderCells,
  });

  final int row;
  final List<String> cells;
  final List<TerminalVtWorkerRenderCell> renderCells;

  factory TerminalVtWorkerRowDelta._fromMessage(
    List<Object?> message,
    Map<int, TerminalVtWorkerCellStyle> styles,
  ) {
    return TerminalVtWorkerRowDelta(
      row: message[0]! as int,
      cells: List<String>.unmodifiable((message[1]! as List).cast<String>()),
      renderCells: List<TerminalVtWorkerRenderCell>.unmodifiable(
        (message[2]! as List).map(
          (cell) => TerminalVtWorkerRenderCell._fromMessage(
            (cell! as List).cast<Object?>(),
            styles,
          ),
        ),
      ),
    );
  }
}

/// Incremental viewport update produced without formatting the full terminal.
final class TerminalVtWorkerDelta {
  const TerminalVtWorkerDelta({
    required this.revision,
    required this.cols,
    required this.rows,
    required this.fullRepaint,
    required this.changedRows,
    required this.cursorVisible,
    required this.cursorX,
    required this.cursorY,
    required this.modes,
    required this.styles,
    this.effects = const <TerminalVtWorkerEffect>[],
  });

  final int revision;
  final int cols;
  final int rows;
  final bool fullRepaint;
  final List<TerminalVtWorkerRowDelta> changedRows;
  final bool cursorVisible;
  final int? cursorX;
  final int? cursorY;
  final TerminalVtWorkerModes modes;
  final Map<int, TerminalVtWorkerCellStyle> styles;
  final List<TerminalVtWorkerEffect> effects;

  factory TerminalVtWorkerDelta._fromMessage(List<Object?> message) {
    if (message.length < 9 || message[0] != true) {
      throw StateError('Invalid terminal VT worker delta.');
    }
    final styles = <int, TerminalVtWorkerCellStyle>{};
    if (message.length > 11 && message[11] is List) {
      for (final entry in message[11]! as List) {
        final pair = (entry! as List).cast<Object?>();
        styles[pair[0]! as int] = TerminalVtWorkerCellStyle._fromMessage(
          (pair[1]! as List).cast<Object?>(),
        );
      }
    }
    final immutableStyles = Map<int, TerminalVtWorkerCellStyle>.unmodifiable(
      styles,
    );
    return TerminalVtWorkerDelta(
      revision: message[1]! as int,
      cols: message[2]! as int,
      rows: message[3]! as int,
      fullRepaint: message[4]! as bool,
      changedRows: List<TerminalVtWorkerRowDelta>.unmodifiable(
        (message[5]! as List).map(
          (row) => TerminalVtWorkerRowDelta._fromMessage(
            (row! as List).cast<Object?>(),
            immutableStyles,
          ),
        ),
      ),
      cursorVisible: message[6]! as bool,
      cursorX: message[7] as int?,
      cursorY: message[8] as int?,
      effects: _effectsFromMessage(message.length > 9 ? message[9] : null),
      modes: TerminalVtWorkerModes._fromMessage(
        (message[10]! as List).cast<Object?>(),
      ),
      styles: immutableStyles,
    );
  }
}

/// Owns a Ghostty VT terminal on a dedicated Dart isolate.
///
/// This is intentionally not wired into the xterm renderer yet. It establishes
/// the isolation boundary first so the renderer can migrate incrementally while
/// the existing terminal remains the correctness fallback.
final class TerminalVtWorker {
  TerminalVtWorker._(this._isolate, this._commands);

  final Isolate _isolate;
  final SendPort _commands;
  bool _closed = false;

  static Future<TerminalVtWorker> start({
    required int cols,
    required int rows,
    int maxScrollback = 10_000,
  }) async {
    final ready = ReceivePort();
    final isolate = await Isolate.spawn<List<Object?>>(
      terminalVtWorkerMain,
      <Object?>[ready.sendPort, cols, rows, maxScrollback],
      debugName: 'alera-terminal-vt-worker',
    );
    try {
      final message = await ready.first.timeout(const Duration(seconds: 10));
      if (message is! List || message.isEmpty) {
        isolate.kill(priority: Isolate.immediate);
        throw StateError('Terminal VT worker returned an invalid handshake.');
      }
      if (message[0] == _workerError) {
        isolate.kill(priority: Isolate.immediate);
        throw StateError('Terminal VT worker failed: ${message[1]}');
      }
      if (message[0] != _workerReady || message[1] is! SendPort) {
        isolate.kill(priority: Isolate.immediate);
        throw StateError('Terminal VT worker did not become ready.');
      }
      return TerminalVtWorker._(isolate, message[1]! as SendPort);
    } finally {
      ready.close();
    }
  }

  Future<TerminalVtWorkerSnapshot> writeBytes(Uint8List bytes) {
    final payload = TransferableTypedData.fromList(<Uint8List>[bytes]);
    return _request(<Object?>[_workerWrite, payload]);
  }

  Future<TerminalVtWorkerDelta> writeDelta(Uint8List bytes) async {
    final payload = TransferableTypedData.fromList(<Uint8List>[bytes]);
    return TerminalVtWorkerDelta._fromMessage(
      await _requestRaw(<Object?>[_workerWriteDelta, payload]),
    );
  }

  Future<TerminalVtWorkerSnapshot> resize({
    required int cols,
    required int rows,
  }) {
    return _request(<Object?>[_workerResize, cols, rows]);
  }

  Future<TerminalVtWorkerSnapshot> _request(List<Object?> command) async {
    return TerminalVtWorkerSnapshot._fromMessage(await _requestRaw(command));
  }

  Future<List<Object?>> _requestRaw(List<Object?> command) async {
    if (_closed) {
      throw StateError('Terminal VT worker is already closed.');
    }
    final reply = ReceivePort();
    try {
      _commands.send(<Object?>[command[0], reply.sendPort, ...command.skip(1)]);
      final response = await reply.first;
      if (response is! List || response.isEmpty) {
        throw StateError('Terminal VT worker returned an invalid response.');
      }
      if (response[0] != true) {
        throw StateError('Terminal VT worker failed: ${response[1]}');
      }
      return response.cast<Object?>();
    } finally {
      reply.close();
    }
  }

  Future<void> close() async {
    if (_closed) {
      return;
    }
    _closed = true;
    final reply = ReceivePort();
    try {
      _commands.send(<Object?>[_workerClose, reply.sendPort]);
      await reply.first.timeout(const Duration(seconds: 5));
    } finally {
      reply.close();
      _isolate.kill(priority: Isolate.immediate);
    }
  }
}

/// Worker isolate entry point. Kept public so `Isolate.spawn` remains valid in
/// AOT builds where entry points are tree-shaken aggressively.
@pragma('vm:entry-point')
void terminalVtWorkerMain(List<Object?> initialization) {
  final owner = initialization[0]! as SendPort;
  final commands = ReceivePort();
  VtTerminal? terminal;
  VtTerminalFormatter? formatter;
  VtRenderState? renderState;
  final pendingEffects = <Object?>[];
  var revision = 0;

  try {
    final createdTerminal = VtTerminal(
      cols: initialization[1]! as int,
      rows: initialization[2]! as int,
      maxScrollback: initialization[3]! as int,
    );
    terminal = createdTerminal;
    formatter = createdTerminal.createFormatter(
      const VtFormatterTerminalOptions(trim: false),
    );
    renderState = createdTerminal.createRenderState();
    createdTerminal.onWritePty = (data) {
      pendingEffects.add(<Object?>[_effectPtyWrite, Uint8List.fromList(data)]);
    };
    createdTerminal.onTitleChanged = () {
      pendingEffects.add(<Object?>[_effectTitleChanged, createdTerminal.title]);
    };
    createdTerminal.onBell = () {
      pendingEffects.add(const <Object?>[_effectBell]);
    };
  } catch (error, stackTrace) {
    owner.send(<Object?>[
      _workerError,
      error.toString(),
      stackTrace.toString(),
    ]);
    commands.close();
    return;
  }

  void clearDirty() {
    renderState!.visitRows((row) {
      if (row.dirty) {
        row.dirty = false;
      }
    });
    renderState.dirty = ghostty_bindings
        .GhosttyRenderStateDirty
        .GHOSTTY_RENDER_STATE_DIRTY_FALSE;
  }

  List<Object?> interactionModes() {
    final currentTerminal = terminal!;
    final mouse = currentTerminal.mouseProtocolState;
    return <Object?>[
      currentTerminal.getMode(VtModes.bracketedPaste),
      currentTerminal.getMode(VtModes.cursorKeys),
      currentTerminal.getMode(VtModes.keypadKeys),
      currentTerminal.cursorVisible,
      mouse.enabled,
      mouse.trackingMode?.value,
      mouse.format?.value,
      mouse.focusEvents,
      mouse.altScroll,
    ];
  }

  List<Object?> snapshot() {
    renderState!.update();
    final render = renderState.snapshot();
    final rows = <String>[];
    for (final row in render.rowsData) {
      final text = StringBuffer();
      for (final cell in row.cells) {
        text.write(cell.graphemes);
      }
      rows.add(text.toString());
    }
    final cursor = render.cursor;
    final message = <Object?>[
      true,
      revision,
      render.cols,
      render.rows,
      formatter!.formatText(),
      rows,
      cursor.visible,
      cursor.hasViewportPosition ? cursor.viewportX : null,
      cursor.hasViewportPosition ? cursor.viewportY : null,
      List<Object?>.from(pendingEffects),
      interactionModes(),
    ];
    clearDirty();
    return message;
  }

  int? packedRgb(VtRgbColor? color) {
    if (color == null) {
      return null;
    }
    return (color.r << 16) | (color.g << 8) | color.b;
  }

  List<Object?> serializeStyleColor(VtStyleColor color) {
    return <Object?>[color.tag.value, color.paletteIndex, packedRgb(color.rgb)];
  }

  List<Object?> serializeStyle(VtStyle style) {
    return <Object?>[
      serializeStyleColor(style.foreground),
      serializeStyleColor(style.background),
      serializeStyleColor(style.underlineColor),
      style.bold,
      style.italic,
      style.faint,
      style.blink,
      style.inverse,
      style.invisible,
      style.strikethrough,
      style.overline,
      style.underline.value,
    ];
  }

  int cellWidth(VtCellSnapshot cell) {
    return switch (cell.wide.value) {
      0 => 1,
      1 => 2,
      _ => 0,
    };
  }

  List<Object?> rowCells(
    VtRenderRowCursor row,
    Map<int, List<Object?>> styles,
  ) {
    final cells = <String>[];
    final renderCells = <Object?>[];
    row.visitCells((cursor) {
      while (cursor.moveNext()) {
        final cell = cursor.current;
        final raw = cell.raw;
        final styleId = raw.styleId;
        styles.putIfAbsent(styleId, () => serializeStyle(cell.style));
        cells.add(cell.graphemes);
        renderCells.add(<Object?>[
          cell.graphemes,
          cellWidth(raw),
          raw.wide.value,
          raw.hasText,
          raw.hasStyling,
          styleId,
          raw.hasHyperlink,
          raw.isProtected,
          raw.semanticContent.value,
          raw.colorPaletteIndex,
          packedRgb(raw.colorRgb),
        ]);
      }
    });
    return <Object?>[cells, renderCells];
  }

  List<Object?> delta() {
    renderState!.update();
    final dirty = renderState.dirty;
    final fullRepaint =
        dirty ==
        ghostty_bindings
            .GhosttyRenderStateDirty
            .GHOSTTY_RENDER_STATE_DIRTY_FULL;
    final changedRows = <Object?>[];
    final styles = <int, List<Object?>>{};
    var rowIndex = 0;
    renderState.visitRows((row) {
      if (fullRepaint || row.dirty) {
        final cells = rowCells(row, styles);
        changedRows.add(<Object?>[rowIndex, cells[0], cells[1]]);
      }
      if (row.dirty) {
        row.dirty = false;
      }
      rowIndex += 1;
    });
    final cursor = renderState.cursorSnapshot;
    final message = <Object?>[
      true,
      revision,
      renderState.cols,
      renderState.rows,
      fullRepaint,
      changedRows,
      cursor.visible,
      cursor.hasViewportPosition ? cursor.viewportX : null,
      cursor.hasViewportPosition ? cursor.viewportY : null,
      List<Object?>.from(pendingEffects),
      interactionModes(),
      styles.entries
          .map((entry) => <Object?>[entry.key, entry.value])
          .toList(growable: false),
    ];
    renderState.dirty = ghostty_bindings
        .GhosttyRenderStateDirty
        .GHOSTTY_RENDER_STATE_DIRTY_FALSE;
    return message;
  }

  void fail(SendPort reply, Object error, StackTrace stackTrace) {
    reply.send(<Object?>[false, error.toString(), stackTrace.toString()]);
  }

  commands.listen((Object? message) {
    if (message is! List || message.length < 2 || message[1] is! SendPort) {
      return;
    }
    final reply = message[1]! as SendPort;
    try {
      switch (message[0]) {
        case _workerWrite:
          pendingEffects.clear();
          final transferable = message[2]! as TransferableTypedData;
          terminal!.writeBytes(transferable.materialize().asUint8List());
          revision += 1;
          reply.send(snapshot());
        case _workerWriteDelta:
          pendingEffects.clear();
          final transferable = message[2]! as TransferableTypedData;
          terminal!.writeBytes(transferable.materialize().asUint8List());
          revision += 1;
          reply.send(delta());
        case _workerResize:
          pendingEffects.clear();
          terminal!.resize(cols: message[2]! as int, rows: message[3]! as int);
          revision += 1;
          reply.send(snapshot());
        case _workerClose:
          renderState!.close();
          formatter!.close();
          terminal!.close();
          reply.send(const <Object?>[true]);
          commands.close();
      }
    } catch (error, stackTrace) {
      fail(reply, error, stackTrace);
    }
  });

  owner.send(<Object?>[_workerReady, commands.sendPort]);
}
