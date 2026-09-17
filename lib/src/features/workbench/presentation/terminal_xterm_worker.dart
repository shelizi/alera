import 'dart:isolate';

import 'package:xterm2/xterm.dart';

const String _workerReady = 'ready';
const String _workerWrite = 'write';
const String _workerResize = 'resize';
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

  Future<TerminalXtermWorkerSnapshot> resize({
    required int cols,
    required int rows,
  }) {
    return _request(<Object?>[_workerResize, cols, rows]);
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

  List<Object?> snapshot() {
    final first = terminal.buffer.scrollBack;
    final viewportRows = <String>[
      for (var row = 0; row < terminal.viewHeight; row++)
        terminal.buffer.lines[first + row].toString(),
    ];
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
          reply.send(snapshot());
        case _workerResize:
          effects.clear();
          terminal.resize(raw[2]! as int, raw[3]! as int);
          reply.send(snapshot());
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
