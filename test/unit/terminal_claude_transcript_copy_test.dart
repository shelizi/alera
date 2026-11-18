import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/presentation/terminal_claude_transcript_copy.dart';
import 'package:alera/src/features/workbench/presentation/terminal_code_block.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  const workspacePath = r'D:\sandbox\queens_demo';
  const fullCode =
      '#include <stdio.h>\n'
      '\n'
      '#define N 8\n'
      '\n'
      'static int queens[N];\n'
      'static int solution_count = 0;\n'
      '\n'
      'int is_safe(int row, int col)\n'
      '{\n'
      '  if (row == N) {\n'
      '    return 0;\n'
      '  }\n'
      '  return 1;\n'
      '}\n'
      '\n'
      'void solve(int row)\n'
      '{\n'
      '  if (is_safe(row, 0)) {\n'
      '    queens[row] = 0;\n'
      '  }\n'
      '}\n'
      '\n'
      'int main(void)\n'
      '{\n'
      '  solve(0);\n'
      '  printf("Done");\n'
      '  return 0;\n'
      '}';

  const visible =
      '#include <stdio.h>\n'
      '\n'
      '#define N 8\n'
      '\n'
      'static int queens[N];\n'
      'static int solution_count = 0;\n'
      '\n'
      'int is_safe(int row, int col)\n'
      '{\n'
      '  if (row == N) {\n'
      '    return 0;\n'
      '  }\n'
      '  return 1;                          Jump to bottom (ctrl+End)';

  test('recovers complete eight-queens source beyond TUI viewport', () {
    expect(
      recoverCompleteClaudeSourceFromText(
        visibleCode: visible,
        assistantText:
            fullCode + '\n\nHow it works: For N = 8, use backtracking...',
      ),
      fullCode,
    );
  });

  test('never substitutes an unrelated assistant code block', () {
    expect(
      recoverCompleteClaudeSourceFromText(
        visibleCode: visible,
        assistantText: fullCode.replaceFirst(
          'static int queens[N];',
          'static int other[N];',
        ),
      ),
      isNull,
    );
    expect(
      recoverCompleteClaudeSourceFromText(
        visibleCode: 'printf("hello");',
        assistantText: fullCode,
      ),
      isNull,
    );
  });

  test(
    'extracts only exact-workspace canonical blocks from manual Claude JSONL',
    () async {
      final root = await Directory.systemTemp.createTemp('alera-raw-jsonl-');
      addTearDown(() async {
        if (await root.exists()) await root.delete(recursive: true);
      });
      final projectName = workspacePath.replaceAll(
        RegExp(r'[^A-Za-z0-9-]'),
        '-',
      );
      final project = Directory(p.join(root.path, projectName))
        ..createSync(recursive: true);
      final file = File(p.join(project.path, 'claude-session.jsonl'));
      final swift = 'let answer = 42\nprint(answer)';
      final objc =
          '#import <Foundation/Foundation.h>\n'
          'int main(void) { return 0; }';
      final markdown =
          '\u0060\u0060\u0060swift\n' +
          swift +
          '\n\u0060\u0060\u0060\n\n' +
          '\u0060\u0060\u0060objective-c\n' +
          objc +
          '\n\u0060\u0060\u0060';
      final valid = {
        'type': 'assistant',
        'cwd': workspacePath,
        'message': {
          'id': 'message-1',
          'content': [
            {'type': 'text', 'text': markdown},
          ],
        },
      };
      await file.writeAsString(
        [
          jsonEncode({
            'type': 'assistant',
            'cwd': r'D:\sandbox\other_workspace',
            'message': {
              'content': [
                {
                  'type': 'text',
                  'text': '\u0060\u0060\u0060js\nconst bad = true;\n\u0060\u0060\u0060',
                },
              ],
            },
          }),
          jsonEncode(valid),
          jsonEncode(valid),
          '{incomplete record',
        ].join('\n'),
      );

      final blocks = await loadClaudeCodeBlocksFromTranscript(
        workspacePath,
        projectsRoot: root,
      );
      expect(blocks.map((block) => block.code), [swift, objc]);
      expect(
        await loadClaudeCodeBlocksFromTranscript(
          r'D:\sandbox\other_workspace',
          projectsRoot: root,
        ),
        isEmpty,
        reason: 'Never leak or match an unrelated workspace transcript',
      );
    },
  );

  test('reads only exact same-workspace matching Claude JSONL', () async {
    final root = await Directory.systemTemp.createTemp('alera-claude-copy-');
    addTearDown(() async {
      if (await root.exists()) await root.delete(recursive: true);
    });
    final project = await Directory(p.join(root.path, 'D--sandbox-queens-demo'))
        .create(recursive: true);
    final transcript = File(p.join(project.path, 'session-1.jsonl'));
    await transcript.writeAsString(
      jsonEncode({
            'type': 'assistant',
            'cwd': workspacePath,
            'message': {
              'content': [
                {
                  'type': 'text',
                  'text':
                      fullCode +
                      '\n\nHow it works: For N = 8, use backtracking',
                },
              ],
            },
          }) +
          '\n',
    );
    expect(
      await recoverCompleteClaudeCodeFromTranscript(
        workspacePath: workspacePath,
        visibleCode: visible,
        projectsRoot: root,
      ),
      fullCode,
    );
    final markerFree = visible.replaceAll('Jump to bottom (ctrl+End)', '');
    expect(
      await recoverCompleteClaudeCodeFromTranscript(
        workspacePath: workspacePath,
        visibleCode: markerFree,
        projectsRoot: root,
      ),
      isNull,
      reason: 'No marker evidence means no transcript I/O by default',
    );
    expect(
      await recoverCompleteClaudeCodeFromTranscript(
        workspacePath: workspacePath,
        visibleCode: markerFree,
        viewportHintNearby: true,
        projectsRoot: root,
      ),
      fullCode,
      reason: 'Confirmed adjacent marker permits exact-prefix source recovery',
    );
    expect(
      await recoverCompleteClaudeCodeFromTranscript(
        workspacePath: r'D:\sandbox\someone-else',
        visibleCode: visible,
        projectsRoot: root,
      ),
      isNull,
      reason: 'Never use another workspace transcript',
    );
    await transcript.writeAsString(
      jsonEncode({
            'type': 'assistant',
            'cwd': r'D:\sandbox\someone-else',
            'message': {
              'content': [
                {'type': 'text', 'text': fullCode},
              ],
            },
          }) +
          '\n',
    );
    expect(
      await recoverCompleteClaudeCodeFromTranscript(
        workspacePath: workspacePath,
        visibleCode: visible,
        projectsRoot: root,
      ),
      isNull,
      reason: 'Reject matching code prefix when the JSONL cwd is unrelated',
    );
  });

  test(
    'recovers separate Objective-C and Swift source from a single reply',
    () {
      final objcLines = <String>[
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
        '- (void)solve',
        '{',
        '  for (NSInteger row = 0; row < N; row++) {',
        '    _queens[row] = row;',
        '  }',
        '}',
        '@end',
        '',
        'int main(void) {',
        '  @autoreleasepool {',
        '    QueensSolver *solver = [[QueensSolver alloc] init];',
        '    [solver solve];',
        '  }',
        '  return 0;',
        '}',
      ];
      final swiftLines = <String>[
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
      final objc = objcLines.join('\n');
      final swift = swiftLines.join('\n');
      final plain =
          'Objective-C (queens.m):\n\n' +
          objc +
          '\n\nSwift (queens.swift):\n\n' +
          swift +
          '\n\nHow it works: For N = 8, there are 92 solutions.';
      final fenced =
          '**Objective-C (queens.m)**\n\n' +
          '```objc\n' +
          objc +
          '\n```\n\n' +
          '**Swift (queens.swift)**\n\n```swift\n' +
          swift +
          '\n```\n\nHow it works: Both use backtracking.';
      final partialObjc =
          objcLines.take(16).join('\n') +
          '                Jump to bottom (ctrl+End)';
      final partialSwift =
          swiftLines.take(17).join('\n') +
          '                Jump to bottom (ctrl+End)';
      for (final answer in [plain, fenced]) {
        expect(
          recoverCompleteClaudeSourceFromText(
            visibleCode: partialObjc,
            assistantText: answer,
          ),
          objc,
          reason: 'Obj-C must not include the next Swift code block',
        );
        expect(
          recoverCompleteClaudeSourceFromText(
            visibleCode: partialSwift,
            assistantText: answer,
          ),
          swift,
          reason: 'Swift must not include Obj-C or following explanation',
        );
        expect(
          recoverCompleteClaudeSourceFromText(
            visibleCode: partialSwift.replaceFirst('let n = 8', 'let n = 9'),
            assistantText: answer,
          ),
          isNull,
        );
      }
    },
  );
}
