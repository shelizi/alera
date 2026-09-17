import 'dart:isolate';
import 'dart:typed_data';

import 'package:xterm2/xterm.dart';

const String _workerReady = 'ready';
const String _workerWrite = 'write';
const String _workerWriteDelta = 'writeDelta';
const String _workerResize = 'resize';
const String _workerResizeDelta = 'resizeDelta';
const String _workerClose = 'close';
const String _workerError = 'error';

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
      effects: List<String>.from(message[12]! as List),
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
  final List<String> effects;
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
    );
  }

  final String text;
  final int width;
  final int foreground;
  final int background;
  final int attributes;
  final int underlineColor;
  final int content;
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
      effects: List<String>.from(message[15]! as List),
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
  final List<String> effects;
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
  }) async {
    final ready = ReceivePort();
    final errors = ReceivePort();
    final isolate = await Isolate.spawn<List<Object?>>(
      terminalXtermWorkerMain,
      <Object?>[ready.sendPort, cols, rows, maxLines],
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
  final commands = ReceivePort();
  final effects = <String>[];
  final terminal = Terminal(
    maxLines: maxLines,
    reflowWithHiddenCursor: false,
    onTitleChange: (title) => effects.add('title:$title'),
    onBell: () => effects.add('bell'),
    onOutput: (value) => effects.add('pty:$value'),
  )..resize(cols, rows);
  var revision = 0;
  List<_TerminalXtermWorkerRowCache>? viewportCache;
  var cachedCols = 0;
  var cachedRows = 0;

  List<Object?> renderCellMessage(BufferLine line, int column) {
    final codePoint = line.getCodePoint(column);
    final combining = line.getCombiningCharacters(column);
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
    ];
  }

  List<Object?> rowMessage(int row, BufferLine line) {
    return <Object?>[
      row,
      line.toString(),
      <Object?>[
        for (var column = 0; column < line.length; column++)
          renderCellMessage(line, column),
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
      List<String>.from(effects),
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
      List<String>.from(effects),
      revision,
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
