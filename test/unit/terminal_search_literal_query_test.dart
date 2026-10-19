import 'package:alera/src/features/workbench/domain/terminal_search.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('prepared literal query preserves old case-insensitive matching', () {
    const cases = <(String, String)>[
      ('Needle needle NEEDLE-needle', 'needle'),
      ('aAaAa', 'aa'),
      ('a [literal] match with [literal]', '[literal]'),
      ('one A---a A', 'A'),
      ('café CAFÉ Café', 'fé'),
      ('İ ı i I', 'i'),
      ('Straße STRASSE straße', 'ss'),
      ('😀 中 😀中 😀', '😀'),
      ('', 'x'),
      ('no matches', 'absent'),
      ('line with emoji 😀 and 😀', '😀'),
      ('a b c', ''),
    ];
    for (final (source, needle) in cases) {
      for (final caseSensitive in <bool>[false, true]) {
        final search = TerminalLiteralQuery(
          needle,
          caseSensitive: caseSensitive,
        );
        expect(
          search.matchStarts(source),
          _originalMatchStarts(source, needle, caseSensitive),
          reason: 'source=$source needle=$needle caseSensitive=$caseSensitive',
        );
      }
    }
  });

  test('reused query preserves non-overlapping matches and length', () {
    final search = TerminalLiteralQuery('aa');
    expect(search.matchStarts('aaaaa'), <int>[0, 2]);
    expect(search.matchLength, 2);
    expect(search.matchStarts('zz'), isEmpty);
    // No-match lines must not allocate a match list for every scrollback row.
    expect(
      identical(search.matchStarts('not found'), search.matchStarts('missing')),
      isTrue,
    );
  });

  test('batch terminal search retains match identity and position', () {
    final lines = <TerminalSearchLine>[
      const TerminalSearchLine(id: 11, index: 42, text: 'NO no nO'),
      const TerminalSearchLine(id: 12, index: 43, text: 'notion'),
    ];
    final actual = findTerminalSearchMatches(lines, 'no');
    expect(
      [
        for (final hit in actual)
          (hit.lineId, hit.lineIndex, hit.start, hit.end),
      ],
      <(Object, int, int, int)>[
        (11, 42, 0, 2),
        (11, 42, 3, 5),
        (11, 42, 6, 8),
        (12, 43, 0, 2),
      ],
    );
  });
}

List<int> _originalMatchStarts(String text, String query, bool caseSensitive) {
  if (query.isEmpty) return const <int>[];
  final needle = caseSensitive ? query : query.toLowerCase();
  final haystack = caseSensitive ? text : text.toLowerCase();
  final matches = <int>[];
  var start = 0;
  while (start < haystack.length) {
    final position = haystack.indexOf(needle, start);
    if (position < 0) break;
    matches.add(position);
    start = position + needle.length;
  }
  return matches;
}
