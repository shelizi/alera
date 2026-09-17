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
  });

  final int revision;
  final int cols;
  final int rows;
  final String text;
  final List<String> viewportRows;
  final bool cursorVisible;
  final int? cursorX;
  final int? cursorY;

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
    );
  }
}

/// One changed viewport row returned by the VT worker.
final class TerminalVtWorkerRowDelta {
  const TerminalVtWorkerRowDelta({required this.row, required this.cells});

  final int row;
  final List<String> cells;

  factory TerminalVtWorkerRowDelta._fromMessage(List<Object?> message) {
    return TerminalVtWorkerRowDelta(
      row: message[0]! as int,
      cells: List<String>.unmodifiable((message[1]! as List).cast<String>()),
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
  });

  final int revision;
  final int cols;
  final int rows;
  final bool fullRepaint;
  final List<TerminalVtWorkerRowDelta> changedRows;
  final bool cursorVisible;
  final int? cursorX;
  final int? cursorY;

  factory TerminalVtWorkerDelta._fromMessage(List<Object?> message) {
    if (message.length < 9 || message[0] != true) {
      throw StateError('Invalid terminal VT worker delta.');
    }
    return TerminalVtWorkerDelta(
      revision: message[1]! as int,
      cols: message[2]! as int,
      rows: message[3]! as int,
      fullRepaint: message[4]! as bool,
      changedRows: List<TerminalVtWorkerRowDelta>.unmodifiable(
        (message[5]! as List).map(
          (row) => TerminalVtWorkerRowDelta._fromMessage(
            (row! as List).cast<Object?>(),
          ),
        ),
      ),
      cursorVisible: message[6]! as bool,
      cursorX: message[7] as int?,
      cursorY: message[8] as int?,
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
  var revision = 0;

  try {
    terminal = VtTerminal(
      cols: initialization[1]! as int,
      rows: initialization[2]! as int,
      maxScrollback: initialization[3]! as int,
    );
    formatter = terminal.createFormatter(
      const VtFormatterTerminalOptions(trim: false),
    );
    renderState = terminal.createRenderState();
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
    ];
    clearDirty();
    return message;
  }

  List<String> rowCells(VtRenderRowCursor row) {
    final cells = <String>[];
    row.visitCells((cursor) {
      while (cursor.moveNext()) {
        cells.add(cursor.current.graphemes);
      }
    });
    return cells;
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
    var rowIndex = 0;
    renderState.visitRows((row) {
      if (fullRepaint || row.dirty) {
        changedRows.add(<Object?>[rowIndex, rowCells(row)]);
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
          final transferable = message[2]! as TransferableTypedData;
          terminal!.writeBytes(transferable.materialize().asUint8List());
          revision += 1;
          reply.send(snapshot());
        case _workerWriteDelta:
          final transferable = message[2]! as TransferableTypedData;
          terminal!.writeBytes(transferable.materialize().asUint8List());
          revision += 1;
          reply.send(delta());
        case _workerResize:
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
