import 'package:code_forge/code_forge.dart' as code_forge;
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('editor regex search compatibility', () {
    test('preserves lookahead matches', () {
      expect(
        code_forge.computeRegexSearchRanges((
          text: 'foobar fooqux foobar',
          query: r'foo(?=bar)',
          caseSensitive: true,
          matchWholeWord: false,
        )),
        equals(<(int, int)>[(0, 3), (14, 17)]),
      );
    });

    test('preserves backreferences', () {
      expect(
        code_forge.computeRegexSearchRanges((
          text: 'foofoo foo barfoofoo',
          query: r'(foo)\1',
          caseSensitive: true,
          matchWholeWord: false,
        )),
        equals(<(int, int)>[(0, 6), (14, 20)]),
      );
    });

    test('returns Unicode scalar offsets instead of UTF-16 offsets', () {
      expect(
        code_forge.computeRegexSearchRanges((
          text: 'a😀x😀z',
          query: '😀',
          caseSensitive: true,
          matchWholeWord: false,
        )),
        equals(<(int, int)>[(1, 2), (3, 4)]),
      );
    });

    test('keeps Dart whole-word boundary behavior', () {
      expect(
        code_forge.computeRegexSearchRanges((
          text: 'foo foo_bar xfoo foo',
          query: 'foo',
          caseSensitive: true,
          matchWholeWord: true,
        )),
        equals(<(int, int)>[(0, 3), (17, 20)]),
      );
    });

    test('replace all preserves lookahead semantics', () {
      expect(
        code_forge.computeReplaceAllText((
          text: 'foobar fooqux foobar',
          query: r'foo(?=bar)',
          replacement: 'X',
          isRegex: true,
          caseSensitive: true,
          matchWholeWord: false,
        )),
        'Xbar fooqux Xbar',
      );
    });
  });
}
