import 'dart:math';

import 'package:alera/src/features/agent_status/infra/agent_hook_text_preview.dart';
import 'package:flutter_test/flutter_test.dart';

String _legacySingle(String text, int limit) {
  final normalized = text.trim().replaceAll(
    RegExp(r'[\r\n\u2028\u2029]+'),
    ' ',
  );
  return normalized.length <= limit
      ? normalized
      : normalized.substring(0, limit);
}

String _legacyMultiline(String text, int limit) {
  final normalized = text
      .trim()
      .replaceAll('\r\n', '\n')
      .replaceAll('\r', '\n')
      .replaceAll(RegExp(r'[\u2028\u2029]'), '\n')
      .replaceAll(RegExp(r'\n{3,}'), '\n\n');
  return normalized.length <= limit
      ? normalized
      : normalized.substring(0, limit);
}

void main() {
  void check(String input, int limit) {
    expect(
      normalizeAgentHookSingleLine(input, limit),
      _legacySingle(input, limit),
    );
    expect(
      normalizeAgentHookMultiline(input, limit),
      _legacyMultiline(input, limit),
    );
  }

  test('preserves all whitespace and UTF-16 truncation boundaries', () {
    for (final input in [
      '',
      ' \t\r\n\u2028\u2029 ',
      '\u0085\u00a0\ufeff界😀\u3000',
      'a\r\nb\rc\nd\u2028e\u2029f',
      'a\r\r\n\n\u2028\u2029\r\nb',
      ' a \r\n \t\r\n b ',
      '😀😀\r\n界e\u0301',
      'a\u0085b\u000bc\u000cd',
    ]) {
      for (var limit = 0; limit <= input.length + 2; limit++) {
        check(input, limit);
      }
    }
  });

  test('matches legacy regex normalization for deterministic mixed text', () {
    final random = Random(712403);
    const fragments = [
      'a',
      ' ',
      '\t',
      '\r',
      '\n',
      '\r\n',
      '\u2028',
      '\u2029',
      '界',
      '😀',
      'e\u0301',
      '\u0085',
      '\ufeff',
    ];
    for (var sample = 0; sample < 400; sample++) {
      final text = List.generate(
        random.nextInt(400),
        (_) => fragments[random.nextInt(fragments.length)],
      ).join();
      for (final limit in [0, 1, 2, 7, 160, 200, 8000]) {
        check(text, limit);
      }
    }
  });

  test('large previews preserve collapsed runs before and after the cap', () {
    final log = 'header\r\n' + 'row 界😀\r\n\r\n\r\n' * 60000 + 'tail\n';
    final longRun = 'start' + '\r\n\u2028' * 250000 + 'end😀';
    final plain = 'x' * 1000000;
    for (final text in [log, longRun, plain, ' \t' * 250000 + '界😀']) {
      for (final limit in [1, 5, 6, 7, 160, 200, 8000]) {
        check(text, limit);
      }
    }
  });
}
