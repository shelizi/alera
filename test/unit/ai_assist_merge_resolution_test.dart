import 'dart:convert';

import 'package:alera/src/features/ai_assist/application/ai_assist_merge_resolution.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_errors.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('merge prompt retains all three index-stage versions', () {
    final prompt = buildMergeConflictsPrompt(
      context: const AiAssistMergeConflictContext(
        filePath: 'src/example.dart',
        base: 'base\n',
        ours: 'ours\n',
        theirs: 'theirs\n',
      ),
      customInstructions: 'Keep the public API unchanged.',
    );
    expect(prompt, contains(jsonEncode('base\n')));
    expect(prompt, contains(jsonEncode('ours\n')));
    expect(prompt, contains(jsonEncode('theirs\n')));
    expect(prompt, contains('UNTRUSTED DATA'));
    expect(prompt, contains('Keep the public API unchanged.'));
  });

  test('parses full merged file and explanation', () {
    final json = jsonEncode(<String, String>{
      'merged': 'first\nsecond\n',
      'explanation': 'Kept both paths.',
    });
    final result = parseMergeConflictsSuggestion(json);
    expect(result.merged, 'first\nsecond\n');
    expect(result.explanation, 'Kept both paths.');
  });

  test('rejects malformed output, markers and oversized prompts', () {
    expect(
      () => parseMergeConflictsSuggestion('not JSON'),
      throwsA(isA<AiAssistException>()),
    );
    expect(
      () => parseMergeConflictsSuggestion(
        '{"merged":"<<<<<<< HEAD\nx","explanation":"x"}',
      ),
      throwsA(isA<AiAssistException>()),
    );
    expect(
      () => buildMergeConflictsPrompt(
        context: AiAssistMergeConflictContext(
          filePath: 'large.txt',
          base: 'a' * 60001,
          ours: null,
          theirs: null,
        ),
        customInstructions: '',
      ),
      throwsA(isA<AiAssistException>()),
    );
  });
}
