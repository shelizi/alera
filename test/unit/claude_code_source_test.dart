import 'package:alera/src/features/agent_status/application/claude_code_message_store.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/presentation/terminal_claude_source_code.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm2/xterm.dart' as xterm;

const fence = '\u0060\u0060\u0060';

AgentHookEvent _hook(
  String name,
  Map<String, Object?> extra, {
  String terminal = 'terminal-1',
  String session = 'claude-1',
}) {
  return AgentHookEvent(
    terminalSessionId: terminal,
    workspaceId: 'ws-1',
    tabId: 'tab-1',
    agentType: AgentType.claude,
    hookEventName: name,
    payload: {
      'hook_event_name': name,
      'session_id': session,
      'transcript_path': 'test-transcript-' + session + '.jsonl',
      ...extra,
    },
  );
}

void main() {
  test('source hook assembles ordered deltas and extracts all code blocks', () {
    final store = ClaudeCodeMessageStore();
    addTearDown(store.dispose);
    final source =
        'Explanation before.\n\n' +
        fence +
        'swift\nlet value = 42\nprint(value)\n' +
        fence +
        '\n\nNext language:\n\n' +
        fence +
        'objective-c\n#import <Foundation/Foundation.h>\n'
            'int main(void) { return 0; }\n' +
        fence;
    final split = source.indexOf('Next language');
    store.observeHookEvent(_hook('SessionStart', {}));
    store.observeHookEvent(
      _hook('MessageDisplay', {
        'message_id': 'm-1',
        'index': 1,
        'final': true,
        'delta': source.substring(split),
      }),
    );
    expect(store.blocksForTerminal('terminal-1'), isEmpty);
    store.observeHookEvent(
      _hook('MessageDisplay', {
        'message_id': 'm-1',
        'index': 0,
        'final': false,
        'delta': source.substring(0, split),
      }),
    );
    final blocks = store.blocksForTerminal('terminal-1');
    expect(blocks, hasLength(2));
    expect(blocks.map((block) => block.code), [
      'let value = 42\nprint(value)',
      '#import <Foundation/Foundation.h>\nint main(void) { return 0; }',
    ]);
    store.observeHookEvent(_hook('Stop', {'last_assistant_message': source}));
    expect(
      store.blocksForTerminal('terminal-1'),
      hasLength(2),
      reason: 'Stop must not duplicate the same MessageDisplay source',
    );
  });

  test(
    'Stop provides fallback; different terminals and sessions stay isolated',
    () {
      final store = ClaudeCodeMessageStore();
      addTearDown(store.dispose);
      final source = fence + 'c\nint result = 9;\n' + fence;
      store.observeHookEvent(_hook('Stop', {'last_assistant_message': source}));
      expect(
        store.blocksForTerminal('terminal-1').single.code,
        'int result = 9;',
      );
      expect(store.blocksForTerminal('terminal-2'), isEmpty);
      store.observeHookEvent(_hook('SessionStart', {}, session: 'claude-2'));
      expect(
        store.blocksForTerminal('terminal-1'),
        isEmpty,
        reason: 'A new Claude session cannot inherit earlier source',
      );
      store.observeHookEvent(
        _hook(
          'Stop',
          {'last_assistant_message': source},
          terminal: 'terminal-2',
          session: 'claude-2',
        ),
      );
      expect(store.blocksForTerminal('terminal-2'), hasLength(1));
      store.clearTerminal('terminal-2');
      expect(store.hasClaudeSession('terminal-2'), isFalse);
    },
  );

  test(
    'assembles thousands of out-of-order chunks and ignores duplicate size',
    () {
      final store = ClaudeCodeMessageStore();
      addTearDown(store.dispose);
      store.observeHookEvent(_hook('SessionStart', {}));
      const count = 8192;
      final chunks = List<String>.generate(
        count,
        (index) => 'value_$index = "界😀";\n',
      );
      chunks[0] = fence + 'text\n' + chunks[0];
      chunks[count - 1] += fence;
      void send(int index, String text, {bool isFinal = false}) {
        store.observeHookEvent(
          _hook('MessageDisplay', {
            'message_id': 'fragmented',
            'index': index,
            'final': isFinal,
            'delta': text,
          }),
        );
      }

      send(count - 1, chunks.last, isFinal: true);
      for (var index = count - 2; index > 0; index--) {
        send(index, chunks[index]);
      }
      // A replay is not a new chunk even if its payload exceeds the budget.
      send(1, 'x' * (1024 * 1024 + 1));
      expect(store.blocksForTerminal('terminal-1'), isEmpty);
      send(0, chunks.first);
      expect(
        store.blocksForTerminal('terminal-1').single.code,
        List<String>.generate(
          count,
          (index) => 'value_$index = "界😀";',
        ).join('\n'),
      );
    },
  );

  test(
    'oversized chunks are discarded without poisoning the next assembly',
    () {
      final store = ClaudeCodeMessageStore();
      addTearDown(store.dispose);
      void send(int index, String text, {bool isFinal = false}) {
        store.observeHookEvent(
          _hook('MessageDisplay', {
            'message_id': 'retry',
            'index': index,
            'final': isFinal,
            'delta': text,
          }),
        );
      }

      send(0, 'x' * (1024 * 1024 + 1));
      expect(store.blocksForTerminal('terminal-1'), isEmpty);
      final good = fence + 'text\nrecovered\n' + fence;
      send(0, good, isFinal: true);
      expect(store.blocksForTerminal('terminal-1').single.code, 'recovered');
      // A complete oversized replacement must not replace earlier source.
      send(0, 'x' * (1024 * 1024 + 1), isFinal: true);
      expect(store.blocksForTerminal('terminal-1').single.code, 'recovered');
      send(0, fence + 'text\nnext\n' + fence, isFinal: true);
      expect(store.blocksForTerminal('terminal-1').single.code, 'next');
    },
  );

  test('unfenced prose is never guessed to be executable source', () {
    final store = ClaudeCodeMessageStore();
    addTearDown(store.dispose);
    store.observeHookEvent(
      _hook('Stop', {
        'last_assistant_message':
            'This is ordinary prose.\n'
            'Some code-like text: value = 7\n'
            'There is no Markdown code block.',
      }),
    );
    expect(store.blocksForTerminal('terminal-1'), isEmpty);
  });

  test('matches ANSI-rendered TUI source to exact canonical block', () {
    final store = ClaudeCodeMessageStore();
    addTearDown(store.dispose);
    final source =
        'Here is the program:\n\n' +
        fence +
        'swift\nlet answer = 42\nprint(answer)\n' +
        fence +
        '\n\nAn explanation after it.';
    store.observeHookEvent(
      _hook('MessageDisplay', {
        'message_id': 'm-1',
        'index': 0,
        'final': true,
        'delta': source,
      }),
    );
    final terminal = xterm.Terminal()..resize(100, 15);
    terminal.write(
      '❯ example\r\n'
      '• \x1b[34mlet\x1b[0m answer = \x1b[32m42\x1b[0m\r\n'
      '  print(answer)\r\n'
      '\r\n'
      '• An explanation after it.\r\n',
    );
    final code = resolveClaudeSourceCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(2, 1),
      sourceBlocks: store.blocksForTerminal('terminal-1'),
    );
    expect(code?.code, 'let answer = 42\nprint(answer)');
    expect(
      resolveClaudeSourceCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(2, 4),
        sourceBlocks: store.blocksForTerminal('terminal-1'),
      ),
      isNull,
    );
  });

  test(
    'virtualized output matches two lines but copies full original block',
    () {
      final store = ClaudeCodeMessageStore();
      addTearDown(store.dispose);
      final full = List.generate(70, (i) {
        final n = i + 1;
        return 'value_' + n.toString().padLeft(3, '0') + ' = ' + n.toString();
      }).join('\n');
      store.observeHookEvent(
        _hook('Stop', {
          'last_assistant_message': fence + 'python\n' + full + '\n' + fence,
        }),
      );
      final terminal = xterm.Terminal()..resize(100, 15);
      terminal.write(
        '• value_001 = 1\r\n'
        '  value_002 = 2\r\n'
        '  value_003 = 3\r\n'
        '         Jump to bottom\r\n',
      );
      final code = resolveClaudeSourceCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(5, 1),
        sourceBlocks: store.blocksForTerminal('terminal-1'),
      );
      expect(code?.code, full);
      expect(code?.code.split('\n'), hasLength(70));
    },
  );

  test('canonical block extent reaches the true final visible row', () {
    final store = ClaudeCodeMessageStore();
    addTearDown(store.dispose);
    final sourceLines = List<String>.generate(
      20,
      (index) =>
          'value_' + (index + 1).toString() + ' = ' + (index + 1).toString(),
    );
    store.observeHookEvent(
      _hook('Stop', {
        'last_assistant_message':
            fence + 'python\n' + sourceLines.join('\n') + '\n' + fence,
      }),
    );
    final terminal = xterm.Terminal()..resize(120, 30);
    terminal.write(
      [
        '❯ test',
        '• ' + sourceLines.first,
        for (final line in sourceLines.skip(1)) '  ' + line,
        '',
        'After the program.',
      ].join('\r\n'),
    );
    final block = resolveClaudeSourceCodeBlockAt(
      terminal: terminal,
      offset: const xterm.CellOffset(3, 2),
      sourceBlocks: store.blocksForTerminal('terminal-1'),
    );
    expect(block?.openingRow, 1);
    expect(
      block?.closingRow,
      20,
      reason: 'A hover near the start still knows the real last row',
    );
    expect(block?.code, sourceLines.join('\n'));
  });

  test(
    '80-line canonical source matches after xterm scrollback exceeds viewport',
    () {
      final store = ClaudeCodeMessageStore();
      addTearDown(store.dispose);
      final lines = List<String>.generate(
        80,
        (i) =>
            'line_' +
            (i + 1).toString().padLeft(3, '0') +
            ' = "ALERA_LONG_' +
            (i + 1).toString().padLeft(3, '0') +
            '"',
      );
      final source = lines.join('\n');
      store.observeHookEvent(
        _hook('Stop', {
          'last_assistant_message': fence + 'python\n' + source + '\n' + fence,
        }),
      );
      expect(store.blocksForTerminal('terminal-1').single.code, source);
      final terminal = xterm.Terminal()..resize(120, 25);
      terminal.write(
        [
          '❯ long source',
          '• ' + lines.first,
          for (final line in lines.skip(1)) line,
          '',
          'Finished generating 80 lines.',
        ].join('\r\n'),
      );

      var lastVisibleCode = -1;
      for (var i = 0; i < terminal.buffer.lines.length; i++) {
        if (terminal.buffer.lines[i].getText().contains('ALERA_LONG_075')) {
          lastVisibleCode = i;
          break;
        }
      }
      expect(lastVisibleCode, greaterThan(25));
      final block = resolveClaudeSourceCodeBlockAt(
        terminal: terminal,
        offset: xterm.CellOffset(8, lastVisibleCode),
        sourceBlocks: store.blocksForTerminal('terminal-1'),
      );
      expect(block?.code, source);
      expect(block?.closingRow, greaterThanOrEqualTo(lastVisibleCode));
    },
  );

  test('matches Claude TUI soft-wrapped long source lines exactly', () {
    final store = ClaudeCodeMessageStore();
    addTearDown(store.dispose);
    final source =
        'const longLine = "ALERA_WRAP_010_' +
        List.filled(30, '0123456789abcdef').join() +
        '";';
    store.observeHookEvent(
      _hook('Stop', {
        'last_assistant_message':
            fence + 'typescript\n' + source + '\n' + fence,
      }),
    );
    final terminal = xterm.Terminal()..resize(120, 20);
    final pieces = <String>[];
    for (var i = 0; i < source.length; i += 80) {
      final last = (i + 80).clamp(0, source.length);
      pieces.add(source.substring(i, last));
    }
    terminal.write(
      [
        '❯ show one long line',
        '• ' + pieces.first,
        for (final piece in pieces.skip(1)) '  ' + piece,
        '',
        'Explanation, not code',
      ].join('\r\n'),
    );
    for (final row in <int>[1, 2, 4, pieces.length]) {
      final result = resolveClaudeSourceCodeBlockAt(
        terminal: terminal,
        offset: xterm.CellOffset(7, row),
        sourceBlocks: store.blocksForTerminal('terminal-1'),
      );
      expect(
        result?.code,
        source,
        reason:
            'Source line should match when hovered at rendered row ' +
            row.toString(),
      );
      expect(result?.openingRow, 1);
      expect(result?.closingRow, pieces.length);
    }
  });

  test(
    'preserves nested triple-backticks inside a four-backtick source fence',
    () {
      final store = ClaudeCodeMessageStore();
      addTearDown(store.dispose);
      final tick = String.fromCharCode(96);
      final triple = tick * 3;
      final quadruple = tick * 4;
      final code = [
        'ALERA_NESTED_START',
        triple,
        '',
        'ALERA_NESTED_INNER',
        triple,
        'ALERA_NESTED_END',
      ].join('\n');
      store.observeHookEvent(
        _hook('Stop', {
          'last_assistant_message':
              quadruple + 'text\n' + code + '\n' + quadruple,
        }),
      );
      final blocks = store.blocksForTerminal('terminal-1');
      expect(blocks, hasLength(1));
      expect(blocks.single.code, code);
      final terminal = xterm.Terminal()..resize(110, 18);
      terminal.write(
        [
          '❯ source',
          '• ALERA_NESTED_START',
          '  ' + triple,
          '',
          '  ALERA_NESTED_INNER',
          '  ' + triple,
          '  ALERA_NESTED_END',
          '',
          'Finished.',
        ].join('\r\n'),
      );
      for (final row in <int>[1, 4, 6]) {
        final match = resolveClaudeSourceCodeBlockAt(
          terminal: terminal,
          offset: xterm.CellOffset(8, row),
          sourceBlocks: blocks,
        );
        expect(match?.code, code);
        expect(match?.openingRow, 1);
        expect(match?.closingRow, 6);
      }
    },
  );

  test('rejects code snippets when only a shared prefix matches', () {
    final store = ClaudeCodeMessageStore();
    addTearDown(store.dispose);
    store.observeHookEvent(
      _hook('Stop', {
        'last_assistant_message':
            fence +
            'js\n'
                'const answer = 42;\nconsole.log(answer);\n' +
            fence,
      }),
    );
    final terminal = xterm.Terminal()..resize(100, 10);
    terminal.write('const answer = 42;\r\nSome unrelated explanation\r\n');
    expect(
      resolveClaudeSourceCodeBlockAt(
        terminal: terminal,
        offset: const xterm.CellOffset(2, 0),
        sourceBlocks: store.blocksForTerminal('terminal-1'),
      ),
      isNull,
    );
  });
}
