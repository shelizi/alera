import 'dart:convert';
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

  test(
    'streamed JSON previews preserve escaping and every truncation boundary',
    () {
      final values = <Object?>[
        null,
        true,
        false,
        1,
        -1.25,
        {},
        [],
        {
          '界😀\u2028\u2029': [
            'quote" slash\\ \t \r\n',
            String.fromCharCodes([0xd800, 0x61, 0xdc00]),
            'e\u0301\u2028\u2028\u2029尾',
            {'key': true, 'number': 42},
          ],
        },
        _JsonObject({'custom': '界😀\u2028\u2029tail'}),
        'x' * 200 + '\u2028\u2029',
      ];
      for (final value in values) {
        final full = jsonEncode(value);
        for (var limit = 0; limit <= full.length + 1; limit++) {
          expect(
            normalizeAgentHookJsonPreview(value, limit),
            _legacySingle(full, limit),
            reason: 'JSON preview limit $limit',
          );
        }
      }
    },
  );

  test(
    'large JSON input retains exact prefix and validates its unseen suffix',
    () {
      final text = '界😀\r\n\u2028\u2029 "quoted" \\ ' * 28000;
      final valid = {
        'result': text,
        'metadata': List.generate(20000, (index) => index),
      };
      final encoded = jsonEncode(valid);
      for (final limit in [0, 1, 2, 7, 160, 200, 8000]) {
        expect(
          normalizeAgentHookJsonPreview(valid, limit),
          _legacySingle(encoded, limit),
        );
      }

      final cyclic = <Object?>[];
      cyclic.add(cyclic);
      for (final unsupported in <Object?>[
        Object(),
        double.nan,
        double.infinity,
        {1: 'non-string key'},
        cyclic,
        _ThrowingJsonObject(),
      ]) {
        expect(
          normalizeAgentHookJsonPreview({
            'result': text,
            'invalid': unsupported,
          }, 160),
          isNull,
        );
      }
    },
  );

  test('matches legacy JSON preview across randomized nested values', () {
    final random = Random(762);
    Object? makeValue(int depth) {
      switch (random.nextInt(depth == 0 ? 5 : 8)) {
        case 0:
          return null;
        case 1:
          return random.nextBool();
        case 2:
          return random.nextInt(100000);
        case 3:
          return random.nextDouble();
        case 4:
          const pieces = [
            'a',
            ' ',
            '"',
            '\\',
            '\n',
            '😀',
            '\u2028',
            '\u2029',
            '\u0000',
          ];
          return List.generate(
            random.nextInt(200),
            (_) => pieces[random.nextInt(pieces.length)],
          ).join();
        case 5:
          return List.generate(random.nextInt(6), (_) => makeValue(depth - 1));
        default:
          return {
            for (var i = 0; i < random.nextInt(6); i++)
              'key$i': makeValue(depth - 1),
          };
      }
    }

    for (var i = 0; i < 300; i++) {
      final value = makeValue(3);
      final encoded = jsonEncode(value);
      for (final limit in [0, 1, 2, 13, 160, 200, 8000]) {
        expect(
          normalizeAgentHookJsonPreview(value, limit),
          _legacySingle(encoded, limit),
        );
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

class _JsonObject {
  _JsonObject(this.value);
  final Object? value;
  Object? toJson() => value;
}

class _ThrowingJsonObject {
  Object? toJson() => throw StateError('invalid');
}
