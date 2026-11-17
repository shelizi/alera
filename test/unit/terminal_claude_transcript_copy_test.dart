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
}
