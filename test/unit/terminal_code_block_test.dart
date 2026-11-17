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

  test(
    'recognizes actual Claude Code rendered code without Markdown fences',
    () {
      final terminal = xterm.Terminal()..resize(120, 12);
      addTearDown(terminal.dispose);
      terminal.write(
        '❯ Test prompt\r\n'
        '• \x1b[34mconst\x1b[0m answer = \x1b[32m42\x1b[0m;\r\n'
        '  console.log(answer);\r\n'
        '\r\n'
        '✦ Cooked for 4s',
      );

      final block = resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(9, 2),
      );
      expect(block?.openingRow, 1);
      expect(block?.closingRow, 2);
      expect(block?.code, 'const answer = 42;\nconsole.log(answer);');
      expect(
        resolveTerminalCodeBlockAt(
          terminal: terminal,
          offset: const xterm.CellOffset(5, 4),
        ),
        isNull,
      );
    },
  );

  test('does not mistake ordinary Claude bullet prose for code', () {
    final terminal = xterm.Terminal()..resize(120, 10);
    addTearDown(terminal.dispose);
    terminal.write(
      '• The new terminal feature works correctly.\r\n'
      '  It does not need a tool call.\r\n'
      '\r\n'
      '• const answer = 42;',
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(3, 0),
      ),
      isNull,
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(5, 3),
      )?.code,
      'const answer = 42;',
    );
  });

  test('preserves code indentation and internal blank source lines', () {
    final terminal = xterm.Terminal()..resize(120, 12);
    addTearDown(terminal.dispose);
    terminal.write(
      '• function f() {\r\n'
      '    const a = 1;\r\n'
      '  \r\n'
      '    return a;\r\n'
      '  }\r\n'
      '\r\n'
      'Done',
    );
    final block = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(4, 3),
    );
    expect(block?.code, 'function f() {\n  const a = 1;\n\n  return a;\n}');
  });

  test('keeps Claude rendered code blocks separate across prose', () {
    final terminal = xterm.Terminal()..resize(100, 15);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ UI test\r\n'
      '• const first = 1;\r\n'
      '  console.log(first);\r\n'
      '\r\n'
      '  This second block is separate.\r\n'
      '  It is prose, not source code.\r\n'
      '\r\n'
      '  const second = 2;\r\n'
      '  console.log(second);\r\n'
      '\r\n'
      '✻ Cooked for 2s',
    );
    final first = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(4, 2),
    );
    final second = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(4, 8),
    );
    expect(first?.code, 'const first = 1;\nconsole.log(first);');
    expect(second?.openingRow, 7);
    expect(second?.code, 'const second = 2;\nconsole.log(second);');
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(3, 4),
      ),
      isNull,
    );
  });

  test('detects unbulleted code after Claude prose and not after turn end', () {
    final terminal = xterm.Terminal()..resize(100, 12);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ UI test\r\n'
      '• Here is the sample:\r\n'
      '\r\n'
      '  console.log("hello");\r\n'
      '\r\n'
      '✻ Cooked for 1s\r\n'
      '  const unrelated = 10;',
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(4, 3),
      )?.code,
      'console.log("hello");',
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(4, 6),
      ),
      isNull,
    );
  });

  test('collects Claude code opener wrapped before function parenthesis', () {
    final terminal = xterm.Terminal()..resize(12, 10);
    addTearDown(terminal.dispose);
    terminal.write(
      '• console.log(42);\r\n'
      '\r\n'
      '✻ Cooked for 1s',
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(4, 1),
      )?.code,
      'console.log(42);',
    );
  });
}
