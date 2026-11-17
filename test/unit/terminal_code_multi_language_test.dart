import 'package:alera/src/features/workbench/presentation/terminal_code_block.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm2/xterm.dart' as xterm;

void main() {
  final objc = <String>[
    '#import <Foundation/Foundation.h>',
    '',
    '#define N 8',
    '',
    '@interface QueensSolver : NSObject',
    '@property (nonatomic, readonly) NSInteger solutionCount;',
    '- (void)solve;',
    '@end',
    '',
    '@implementation QueensSolver {',
    '  NSInteger _queens[N];',
    '}',
    '- (BOOL)isSafeAtRow:(NSInteger)row column:(NSInteger)col',
    '{',
    '  for (NSInteger r = 0; r < row; r++) {',
    '    NSInteger c = _queens[r];',
    '    if (c == col) { return NO; }',
    '  }',
    '  return YES;',
    '}',
    '',
    '- (void)solve',
    '{',
    '  _solutionCount = 0;',
    '  [self placeRow:0];',
    '}',
    '@end',
    '',
    'int main(int argc, const char *argv[])',
    '{',
    '  @autoreleasepool {',
    '    QueensSolver *solver = [[QueensSolver alloc] init];',
    '    [solver solve];',
    '  }',
    '  return 0;',
    '}',
  ];
  final swift = <String>[
    'let n = 8',
    '',
    'struct QueensSolver {',
    '  private var queens = [Int](repeating: 0, count: n)',
    '  private(set) var solutionCount = 0',
    '',
    '  private func isSafe(row: Int, col: Int) -> Bool {',
    '    for r in 0..<row {',
    '      let c = queens[r]',
    '      if c == col { return false }',
    '    }',
    '    return true',
    '  }',
    '',
    '  private mutating func place(row: Int) {',
    '    for col in 0..<n where isSafe(row: row, col: col) {',
    '      queens[row] = col',
    '      place(row: row + 1)',
    '    }',
    '  }',
    '  mutating func solve() {',
    '    place(row: 0)',
    '  }',
    '}',
    '',
    'var solver = QueensSolver()',
    'solver.solve()',
    'print("Total solutions")',
  ];

  test('Claude two language blocks have independent complete copy targets', () {
    final terminal = xterm.Terminal()..resize(185, 28);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ give me object c and swift code\r\n'
              '❯ 給我 markdown code block\r\n'
              '• Objective-C (queens.m)\r\n'
              '\r\n' +
          objc.join('\r\n') +
          '\r\n'
              '\r\n'
              'Swift (queens.swift)\r\n'
              '\r\n' +
          swift.join('\r\n') +
          '\r\n'
              '\r\n'
              'How it works: Both versions use backtracking.\r\n'
              '✻ Churned for 10s',
    );
    TerminalCodeBlock? at(int row) => resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: xterm.CellOffset(5, row),
    );
    const objcStart = 4;
    final swiftHeader = objcStart + objc.length + 1;
    final swiftStart = swiftHeader + 2;
    for (final i in [0, 4, 12, 23, objc.length - 1]) {
      expect(
        at(objcStart + i)?.code,
        objc.join('\n'),
        reason: 'Objective-C source row $i should copy queens.m only',
      );
    }
    for (final i in [0, 6, 14, 21, swift.length - 1]) {
      expect(
        at(swiftStart + i)?.code,
        swift.join('\n'),
        reason: 'Swift source row $i should copy queens.swift only',
      );
    }
    for (final row in [0, 1, 2, swiftHeader, swiftStart + swift.length + 1]) {
      expect(at(row), isNull, reason: 'header or prose row $row is not code');
    }
  });

  test(
    'Claude colon headings and two-space gutters preserve source nesting',
    () {
      final terminal = xterm.Terminal()..resize(155, 20);
      addTearDown(terminal.dispose);
      terminal.write(
        '❯ Objective-C and Swift\r\n'
        '• Objective-C (queens.m):\r\n'
        '\r\n'
        '  #import <Foundation/Foundation.h>\r\n'
        '  @interface QueensSolver : NSObject\r\n'
        '  - (void)solve;\r\n'
        '  @end\r\n'
        '\r\n'
        '  Swift (queens.swift):\r\n'
        '\r\n'
        '  import Foundation\r\n'
        '  struct QueensSolver {\r\n'
        '    private mutating func solve() {\r\n'
        '      print("solved")\r\n'
        '    }\r\n'
        '  }\r\n'
        '\r\n'
        '  This is an explanation, not source code.\r\n'
        '✻ Churned for 2s',
      );
      TerminalCodeBlock? at(int row) => resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: xterm.CellOffset(5, row),
      );
      expect(
        at(3)?.code,
        '#import <Foundation/Foundation.h>\n@interface QueensSolver : NSObject\n- (void)solve;\n@end',
      );
      expect(at(5)?.code, at(3)?.code);
      expect(
        at(10)?.code,
        'import Foundation\nstruct QueensSolver {\n  private mutating func solve() {\n    print("solved")\n  }\n}',
      );
      expect(at(14)?.code, at(10)?.code);
      for (final r in [1, 2, 8, 9, 17]) {
        expect(at(r), isNull, reason: 'noncode heading/prose row $r');
      }
    },
  );

  test('language headings followed by prose do not enable code copy', () {
    final terminal = xterm.Terminal()..resize(140, 14);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ Compare languages\r\n'
      '• Objective-C (queens.m):\r\n'
      '\r\n'
      '  This version uses a class with a board.\r\n'
      '\r\n'
      '  Swift (queens.swift):\r\n'
      '  This version uses a struct.\r\n'
      '✻ Churned for 3s',
    );
    for (final row in [1, 3, 5, 6]) {
      expect(
        resolveTerminalCodeBlockAt(
          terminal: terminal,
          offset: xterm.CellOffset(5, row),
        ),
        isNull,
      );
    }
  });

  test('sees virtualized TUI hint next to, but outside, Objective-C code', () {
    final terminal = xterm.Terminal()..resize(115, 20);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ Objective-C\r\n'
      '• Objective-C (queens.m)\r\n'
      '#import <Foundation/Foundation.h>\r\n'
      '@interface QueensSolver : NSObject\r\n'
      '- (void)solve;\r\n'
      '@end\r\n'
      'Jump to bottom (ctrl+End)\r\n'
      '✻ Churned for 4s',
    );
    final block = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(3, 3),
    );
    expect(
      block?.code,
      '#import <Foundation/Foundation.h>\n'
      '@interface QueensSolver : NSObject\n'
      '- (void)solve;\n@end',
    );
    expect(block!.code.contains('Jump to bottom'), isFalse);
    expect(
      terminalCodeBlockHasViewportHint(terminal: terminal, block: block),
      isTrue,
    );
  });

  test('old unrelated viewport markers do not affect later source blocks', () {
    final terminal = xterm.Terminal()..resize(115, 20);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ Objective-C\r\n'
              '• Objective-C (queens.m)\r\n'
              '#import <Foundation/Foundation.h>\r\n'
              '@interface QueensSolver : NSObject\r\n'
              '@end\r\n'
              '✻ Churned for 3s\r\n' +
          List<String>.filled(25, 'unrelated output').join('\r\n') +
          '\r\nJump to bottom (ctrl+End)',
    );
    final block = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(3, 3),
    );
    expect(block, isNotNull);
    expect(
      terminalCodeBlockHasViewportHint(terminal: terminal, block: block!),
      isFalse,
    );
  });

  test(
    'Swift filename header remains copyable after 170-line Obj-C output',
    () {
      final terminal = xterm.Terminal()..resize(175, 22);
      addTearDown(terminal.dispose);
      final olderSource = List<String>.filled(
        170,
        '  printf("Objective-C source");',
      );
      terminal.write(
        '❯ Give me Swift and Obj-C\r\n'
                '• Objective-C (queens.m)\r\n'
                '#import <Foundation/Foundation.h>\r\n' +
            olderSource.join('\r\n') +
            '\r\n'
                '\r\n'
                'Swift (queens.swift)\r\n'
                'let n = 8\r\n'
                'struct QueensSolver {\r\n'
                '  private mutating func solve() {\r\n'
                '    print("Solved")\r\n'
                '  }\r\n'
                '}\r\n'
                '✻ Churned for 4s',
      );
      final swiftStart = 1 + 1 + 1 + olderSource.length + 1 + 1;
      final swift = resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: xterm.CellOffset(6, swiftStart + 2),
      );
      expect(
        swift?.code,
        'let n = 8\n'
        'struct QueensSolver {\n'
        '  private mutating func solve() {\n'
        '    print("Solved")\n'
        '  }\n'
        '}',
        reason: 'Long previous code cannot hide next language copy button',
      );
    },
  );

  test('Swift source filename heading works when Claude bullet is evicted', () {
    final terminal = xterm.Terminal()..resize(140, 22);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ 給我 markdown code block\r\n'
      '  Swift (queens.swift)\r\n'
      '\r\n'
      '  let n = 8\r\n'
      '\r\n'
      '  struct QueensSolver {\r\n'
      '      private var queens = [Int](repeating: 0, count: n)\r\n'
      '      private(set) var solutionCount = 0\r\n'
      '\r\n'
      '      private func isSafe(row: Int, col: Int) -> Bool {\r\n'
      '          for r in 0..<row {\r\n'
      '              let c = queens[r]\r\n'
      '              return true\r\n'
      '          }\r\n'
      '      }\r\n'
      '  }\r\n'
      'Jump to bottom (ctrl+End)',
    );
    final start = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(4, 3),
    );
    final middle = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(4, 9),
    );
    expect(start, isNotNull);
    expect(middle?.code, start?.code);
    expect(
      start!.code,
      'let n = 8\n\nstruct QueensSolver {\n'
      '    private var queens = [Int](repeating: 0, count: n)\n'
      '    private(set) var solutionCount = 0\n\n'
      '    private func isSafe(row: Int, col: Int) -> Bool {\n'
      '        for r in 0..<row {\n'
      '            let c = queens[r]\n'
      '            return true\n'
      '        }\n'
      '    }\n'
      '}',
    );
    expect(
      terminalCodeBlockHasViewportHint(terminal: terminal, block: start),
      isTrue,
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(4, 1),
      ),
      isNull,
      reason: 'title remains non-copyable',
    );
  });

  test('keeps one Swift file across two extra TUI spacer rows', () {
    final terminal = xterm.Terminal()..resize(170, 30);
    addTearDown(terminal.dispose);
    final rows = <String>[
      '❯ markdown code block',
      '  Swift (queens.swift)',
      '',
      '  let n = 8',
      '',
      '  struct QueensSolver {',
      '      private var queens = [Int](repeating: 0, count: n)',
      '      private func isSafe(row: Int, col: Int) -> Bool {',
      '          for r in 0..<n {',
      '              let c = queens[r]',
      '              if c == col {',
      '                  return false',
      '              }',
      '',
      '',
      '          }',
      '          return true',
      '      }',
      '',
      '      private mutating func solve() {',
      '          print("solved")',
      '      }',
      '  }',
      '',
      '  var solver = QueensSolver()',
      '  solver.solve()',
      '✻ Churned for 4s',
    ];
    terminal.write(rows.join('\r\n'));
    final lateRow = rows.indexOf('      private mutating func solve() {');
    final block = resolveTerminalCodeBlockAt(
      terminal: terminal,
      offset: xterm.CellOffset(4, lateRow),
    );
    expect(
      block,
      isNotNull,
      reason: 'The end of Swift must remain inside the same copy target',
    );
    expect(block!.code, contains('private mutating func solve()'));
    expect(block.code, startsWith('let n = 8\n'));
    expect(block.code, endsWith('solver.solve()'));
    expect(
      terminalCodeBlockNeedsCanonicalRecovery(terminal: terminal, block: block),
      isFalse,
      reason: 'Balanced complete code does not require disk lookup',
    );
  });

  test('truncated Swift file requests exact matching complete transcript', () {
    final terminal = xterm.Terminal()..resize(170, 24);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ Swift queens\r\n'
      '  Swift (queens.swift)\r\n'
      '\r\n'
      '  let n = 8\r\n'
      '\r\n'
      '  struct QueensSolver {\r\n'
      '      private var queens = [Int](repeating: 0, count: n)\r\n'
      '      private(set) var solutionCount = 0\r\n'
      '\r\n'
      '      private func isSafe(row: Int, col: Int) -> Bool {\r\n'
      '          for r in 0..<row {\r\n'
      '              let c = queens[r]\r\n'
      '\r\n'
      '              if c == col || abs(c - col) == row - r {\r\n'
      '                  return false\r\n'
      '              }',
    );
    final clipped = TerminalCodeBlock(
      openingRow: 3,
      closingRow: 15,
      code:
          'let n = 8\n\n'
          'struct QueensSolver {\n'
          '    private var queens = [Int](repeating: 0, count: n)\n'
          '    private(set) var solutionCount = 0\n\n'
          '    private func isSafe(row: Int, col: Int) -> Bool {\n'
          '        for r in 0..<row {\n'
          '            let c = queens[r]\n\n'
          '            if c == col || abs(c - col) == row - r {\n'
          '                return false\n'
          '            }',
    );
    expect(clipped.code.split('\n'), hasLength(13));
    expect(
      terminalCodeBlockHasViewportHint(terminal: terminal, block: clipped),
      isFalse,
    );
    expect(
      terminalCodeBlockNeedsCanonicalRecovery(
        terminal: terminal,
        block: clipped,
      ),
      isTrue,
      reason: 'Unclosed Swift struct must not silently copy 13 lines',
    );
  });

  test('without Claude context plain Swift heading is not a code anchor', () {
    final terminal = xterm.Terminal()..resize(140, 14);
    addTearDown(terminal.dispose);
    terminal.write(
      '❯ What is Swift\r\n'
      'Swift\r\n'
      'let n = 8\r\n'
      'struct QueensSolver {\r\n'
      '  let count = 0\r\n'
      '}\r\n',
    );
    expect(
      resolveTerminalCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(4, 3),
      ),
      isNull,
    );
  });
}
