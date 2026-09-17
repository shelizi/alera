import 'package:xterm2/xterm.dart' as xterm;

typedef TerminalSearchSourceListener = void Function();

abstract interface class TerminalSearchSource {
  int get height;
  int get viewWidth;
  int get viewHeight;
  Object get bufferIdentity;

  Object lineIdAt(int index);
  String lineTextAt(int index);
  int? lineIndexOf(Object lineId);

  void addListener(TerminalSearchSourceListener listener);
  void removeListener(TerminalSearchSourceListener listener);
}

final class XtermTerminalSearchSource implements TerminalSearchSource {
  const XtermTerminalSearchSource(this.terminal);

  final xterm.Terminal terminal;

  @override
  int get height => terminal.buffer.height;

  @override
  int get viewWidth => terminal.viewWidth;

  @override
  int get viewHeight => terminal.viewHeight;

  @override
  Object get bufferIdentity => terminal.buffer;

  @override
  Object lineIdAt(int index) => terminal.buffer.lines[index];

  @override
  String lineTextAt(int index) => terminal.buffer.lines[index].getText();

  @override
  int? lineIndexOf(Object lineId) {
    if (lineId is! xterm.BufferLine || !lineId.attached) {
      return null;
    }
    final index = lineId.index;
    if (index < 0 || index >= terminal.buffer.height) {
      return null;
    }
    return identical(terminal.buffer.lines[index], lineId) ? index : null;
  }

  @override
  void addListener(TerminalSearchSourceListener listener) {
    terminal.addListener(listener);
  }

  @override
  void removeListener(TerminalSearchSourceListener listener) {
    terminal.removeListener(listener);
  }
}
