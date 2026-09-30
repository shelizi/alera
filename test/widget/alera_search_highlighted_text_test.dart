import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/typography/alera_search_highlighted_text.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('search text matches are case-insensitive and keep source offsets', () {
    final matches = aleraSearchTextMatches('Alpha beta ALPHA', 'alpha');

    expect(matches, hasLength(2));
    expect((matches[0].start, matches[0].end), (0, 5));
    expect((matches[1].start, matches[1].end), (11, 16));
  });

  test('highlighted span distinguishes active and inactive matches', () {
    const baseStyle = TextStyle(color: Colors.white);
    final span = aleraSearchHighlightedSpan(
      text: 'Alpha alpha',
      query: 'alpha',
      baseStyle: baseStyle,
      activeMatchIndex: 1,
    );

    final matchSpans = <TextSpan>[
      for (final child in span.children ?? const <InlineSpan>[])
        if (child is TextSpan && child.text?.toLowerCase() == 'alpha') child,
    ];
    expect(matchSpans, hasLength(2));
    expect(matchSpans[0].style?.backgroundColor, AleraTokens.accentSubtle);
    expect(matchSpans[1].style?.backgroundColor, AleraTokens.accent);
    expect(matchSpans[1].style?.color, AleraTokens.onAccent);
  });

  testWidgets('highlight widget paints the matching text span', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: AleraSearchHighlightedText(
          text: 'Feature Search',
          query: 'search',
        ),
      ),
    );

    final text = tester.widget<Text>(find.text('Feature Search'));
    final root = text.textSpan! as TextSpan;
    final highlighted = root.children!.whereType<TextSpan>().singleWhere(
      (span) => span.text == 'Search',
    );

    expect(highlighted.style?.backgroundColor, AleraTokens.accentSubtle);
  });
}
