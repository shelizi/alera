import 'package:alera/src/features/workbench/presentation/terminal_code_block.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm2/xterm.dart' as xterm;

void main() {
  test('finds fenced Claude code and excludes language fences', () {
    final terminal = xterm.Terminal()..resize(80, 12);
    addTearDown(terminal.dispose);
    terminal.write(
      'Before\r\n```dart\r\nvoid main() {\r\n'
      '  print("hello");\r\n}\r\n```\r\nAfter',
    );

    final block = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(5, 3),
    );
    expect(block, isNotNull);
    expect(block!.openingRow, 1);
    expect(block.closingRow, 5);
    expect(block.code, 'void main() {\n  print("hello");\n}');
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(0, 0),
      ),
      isNull,
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(0, 6),
      ),
      isNull,
    );
  });

  test('does not join separate fenced blocks across prose', () {
    final terminal = xterm.Terminal()..resize(80, 14);
    addTearDown(terminal.dispose);
    terminal.write(
      '```dart\r\nfirst();\r\n```\r\nordinary prose\r\n'
      '```python\r\nsecond()\r\n```',
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(2, 3),
      ),
      isNull,
    );
    final block = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(2, 5),
    );
    expect(block?.code, 'second()');
    expect(block?.openingRow, 4);
  });

  test('requires closing fence and nonempty source', () {
    final terminal = xterm.Terminal()..resize(80, 10);
    addTearDown(terminal.dispose);
    terminal.write('```dart\r\nunfinished();');
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(0, 1),
      ),
      isNull,
    );
    terminal.write('\r\n```');
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(0, 1),
      )?.code,
      'unfinished();',
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(0, -1),
      ),
      isNull,
    );
  });

  test('strips Claude quote gutters but retains source indentation', () {
    final terminal = xterm.Terminal()..resize(80, 8);
    addTearDown(terminal.dispose);
    terminal.write('│ ```python\r\n│ def f():\r\n│     return 1\r\n│ ```');
    final block = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(5, 2),
    );
    expect(block?.code, 'def f():\n    return 1');
  });

  test('joins terminal-wrapped code rows without injecting newlines', () {
    final terminal = xterm.Terminal()..resize(10, 8);
    addTearDown(terminal.dispose);
    terminal.write('```js\r\nabcdefghijklmno\r\n```');
    final block = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(2, 2),
    );
    expect(block?.code, 'abcdefghijklmno');
  });
}
