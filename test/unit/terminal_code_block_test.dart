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

  test('detects Claude multi-language titled code sections without fences', () {
    final terminal = xterm.Terminal()..resize(110, 36);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ 給我多種語言 Hello World\r\n'
      '• 這裡是幾種常見語言的 Hello World:\r\n'
      '\r\n'
      'Python\r\n'
      'print("Hello, World!")\r\n'
      '\r\n'
      'JavaScript (Node.js)\r\n'
      'console.log("Hello, World!");\r\n'
      '\r\n'
      'C\r\n'
      '#include <stdio.h>\r\n'
      '\r\n'
      'int main(void) {\r\n'
      '  printf("Hello, World!\\n");\r\n'
      '  return 0;\r\n'
      '}\r\n'
      '\r\n'
      'Java\r\n'
      'public class Hello {\r\n'
      '  public static void main(String[] args) {\r\n'
      '    System.out.println("Hello, World!");\r\n'
      '  }\r\n'
      '}\r\n'
      '\r\n'
      'PowerShell\r\n'
      'Write-Output "Hello, World!"\r\n'
      '\r\n'
      '你要用哪種語言？我可以幫你建立檔案。\r\n'
      '✻ Brewed for 4s',
    );

    TerminalCodeBlock? codeAt(int row) => resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: xterm.CellOffset(4, row),
    );

    expect(codeAt(4)?.code, 'print("Hello, World!")');
    expect(codeAt(7)?.code, 'console.log("Hello, World!");');
    expect(
      codeAt(10)?.code,
      '#include <stdio.h>\n\nint main(void) {\n  printf("Hello, World!\\n");\n  return 0;\n}',
    );
    expect(codeAt(13)?.openingRow, codeAt(10)?.openingRow);
    expect(
      codeAt(19)?.code,
      'public class Hello {\n  public static void main(String[] args) {\n    System.out.println("Hello, World!");\n  }\n}',
    );
    expect(codeAt(25)?.code, 'Write-Output "Hello, World!"');
    for (final headingOrProse in [1, 3, 6, 9, 16, 17, 23, 24, 26, 27, 28]) {
      expect(
        codeAt(headingOrProse),
        isNull,
        reason:
            'a language heading or prose line is not copyable: $headingOrProse',
      );
    }
  });

  test('does not treat ordinary language labels or prose as source code', () {
    final terminal = xterm.Terminal()..resize(95, 16);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ 語言建議\r\n'
      '• Available languages:\r\n'
      '\r\n'
      'Python\r\n'
      'This language is easy to learn.\r\n'
      '\r\n'
      'JavaScript\r\n'
      'You can use JavaScript for websites.\r\n'
      '\r\n'
      '✻ Brewed for 2s',
    );
    for (final row in [3, 4, 6, 7]) {
      expect(
        resolveTerminalCodeBlockAt(
          terminal: terminal,
          offset: xterm.CellOffset(2, row),
        ),
        isNull,
      );
    }
  });

  test(
    'copies entire unlabelled Claude eight-queens C program across blank lines',
    () {
      final source = <String>[
        '#include <stdio.h>',
        '#include <stdlib.h>',
        '',
        '#define N 8',
        '',
        'static int queens[N];',
        'static int col_used[N];',
        'static int diag1[2 * N - 1];',
        'static int diag2[2 * N - 1];',
        'static int solution_count = 0;',
        '',
        'static void print_board(void) {',
        '  printf("Solution #%d:\\n", solution_count);',
        '  for (int r = 0; r < N; r++) {',
        '    for (int c = 0; c < N; c++) {',
        "      putchar(queens[r] == c ? 'Q' : '.');",
        "      putchar(' ');",
        '    }',
        "    putchar('\\n');",
        '  }',
        '}',
        '',
        'static void solve(int row) {',
        '  if (row == N) {',
        '    solution_count++;',
        '    print_board();',
        '    return;',
        '  }',
        '  for (int col = 0; col < N; col++) {',
        '    int d1 = row + col;',
        '    int d2 = row - col + N - 1;',
        '    if (col_used[col] || diag1[d1] || diag2[d2])',
        '      continue;',
        '    /* 放置皇后 */',
        '    queens[row] = col;',
        '    col_used[col] = diag1[d1] = diag2[d2] = 1;',
        '    solve(row + 1);',
        '    /* 回溯：移除皇后 */',
        '    col_used[col] = diag1[d1] = diag2[d2] = 0;',
        '  }',
        '}',
        '',
        'int main(void) {',
        '  solve(0);',
        '  printf("Total solutions: %d\\n", solution_count);',
        '  return EXIT_SUCCESS;',
        '}',
      ];
      final terminal = xterm.Terminal()..resize(110, 20);
      addTearDown(terminal.dispose);
      terminal.write(
        '❯ 給我八皇后 C implement 的 code\r\n'
        '• 下面是八皇后問題的 C 實作，它用回溯法找出全部 92 組解。\r\n'
        '\r\n'
        '${source.join('\r\n')}\r\n'
        '\r\n'
        '運作方式\r\n'
        '- 每一列放一個皇后。\r\n'
        '編譯與執行\r\n'
        'gcc -O2 -o queens queens.c\r\n'
        './queens\r\n'
        '✻ Crunched for 8s',
      );
      TerminalCodeBlock? at(int row) => resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: xterm.CellOffset(8, row),
      );
      final expected = source.join('\n');
      expect(at(3)?.code, expected, reason: 'include at first line');
      expect(
        at(3 + source.indexOf('static void solve(int row) {'))?.code,
        expected,
        reason: 'hover over nested function after many blank rows',
      );
      expect(
        at(
          3 +
              source.indexOf(
                '  printf("Total solutions: %d\\n", solution_count);',
              ),
        )?.code,
        expected,
        reason: 'hover near end must copy full C file',
      );
      expect(at(2), isNull, reason: 'intro spacer is not code');
      expect(
        at(3 + source.length + 1),
        isNull,
        reason: 'prose heading is not code',
      );
      expect(
        at(3 + source.length + 4),
        isNull,
        reason: 'shell commands are outside code',
      );
    },
  );

  test('does not copy terminal output after Claude turn end', () {
    final terminal = xterm.Terminal()..resize(100, 16);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ Explain C code\r\n'
      '• The code follows:\r\n'
      '\r\n'
      'static int queens[N];\r\n'
      'int main(void) {\r\n'
      '  return 0;\r\n'
      '}\r\n'
      '✻ Crunched for 8s\r\n'
      '#include <not-code>\r\n'
      'int main(void) { return 1; }',
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(2, 9),
      ),
      isNull,
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(2, 5),
      )?.code,
      'static int queens[N];\nint main(void) {\n  return 0;\n}',
    );
  });

  test('copies Claude bullet-start eight-queens C code without fences', () {
    final source = <String>[
      '#include <stdio.h>',
      '',
      '#define N 8',
      '',
      'static int queens[N];',
      'static int solution_count = 0;',
      '',
      'void print_board(void)',
      '{',
      '  printf("Solution %d\\n", solution_count);',
      '}',
      '',
      'void solve(int row)',
      '{',
      '  if (row == N) {',
      '    solution_count++;',
      '    print_board();',
      '    return;',
      '  }',
      '  for (int col = 0; col < N; col++) {',
      '    queens[row] = col;',
      '    solve(row + 1);',
      '  }',
      '}',
      '',
      'int main(void)',
      '{',
      '  solve(0);',
      '  return 0;',
      '}',
    ];
    final terminal = xterm.Terminal()..resize(120, 20);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ Give me 8 queens code\r\n'
      '• ${source.first}\r\n'
      '${source.skip(1).map((line) => line.isEmpty ? "" : "  $line").join('\r\n')}\r\n'
      'How it works: place queens using backtracking.\r\n'
      '✻ Churned for 6s',
    );
    final expected = source.join('\n');
    for (final row in [1, 4, 10, 18, 28]) {
      expect(
        resolveTerminalCodeBlockAt(
          terminal: terminal,
          offset: xterm.CellOffset(3, row),
        )?.code,
        expected,
        reason: 'code should remain a single block at scrollback row $row',
      );
    }
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(3, 31),
      ),
      isNull,
      reason: 'explanation after code is not copyable',
    );
  });
}
