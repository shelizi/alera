import 'package:alera_mobile/src/features/workbench/application/mobile_markdown_preview.dart';
import 'package:test/test.dart';

void main() {
  test('classifies Markdown headings, quotes, lists and fenced source', () {
    final lines = parsePassiveMarkdown(
      '# Hello\n- a\n> quote\n```dart\n# stays code\n```\nbody',
    );
    expect(lines[0].kind, MobileMarkdownLineKind.heading);
    expect(lines[0].text, 'Hello');
    expect(lines[0].level, 1);
    expect(lines[1].kind, MobileMarkdownLineKind.list);
    expect(lines[2].kind, MobileMarkdownLineKind.quote);
    expect(lines[4].kind, MobileMarkdownLineKind.code);
    expect(lines.last.kind, MobileMarkdownLineKind.normal);
    expect(lines.last.text, 'body');
  });

  test('only matching Markdown extensions enable passive preview', () {
    expect(isMarkdownWorkspaceFile('docs/README.MD'), isTrue);
    expect(isMarkdownWorkspaceFile('README.markdown'), isTrue);
    expect(isMarkdownWorkspaceFile('main.dart'), isFalse);
    expect(isMarkdownWorkspaceFile('secrets.md.tmp'), isFalse);
  });
}
