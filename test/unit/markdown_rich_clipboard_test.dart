import 'package:alera/src/features/workbench/infra/markdown_rich_clipboard.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('document exports semantic rich text and a rendered-text fallback', () {
    const markdown = '''# Quarterly 報告

**Bold** and *italic* with [a link](https://example.com?q=1&x=2).

| Name | Score |
| --- | ---: |
| 臺灣 | **98** |

- Entry

```dart
void main() => print('hello');
```
''';
    final data = markdownRichClipboardDocument(markdown);
    expect(data.html, contains('<h1 style='));
    expect(data.html, contains('<strong>Bold</strong>'));
    expect(data.html, contains('<em>italic</em>'));
    expect(data.html, contains('<table style='));
    expect(data.html, contains('<th style='));
    expect(data.html, contains('<td style='));
    expect(data.html, contains('border-collapse:collapse'));
    expect(data.html, contains('<a href="https://example.com'));
    expect(data.html, contains('class="language-dart"'));
    expect(data.plainText, contains('Quarterly 報告'));
    expect(data.plainText, contains('臺灣'));
    expect(data.plainText, isNot(contains('**Bold**')));
    expect(data.plainText, isNot(contains('| ---')));
  });

  test('table copy preserves header cells and inline styling', () {
    final data = markdownRichClipboardTable(const <MarkdownRichTableRow>[
      MarkdownRichTableRow(cells: ['Name', 'Value'], isHeader: true),
      MarkdownRichTableRow(cells: ['<test> & data', '**12**'], isHeader: false),
    ]);
    expect(data.html, contains('<table style='));
    expect(RegExp('<th style=').allMatches(data.html).length, 2);
    expect(RegExp('<td style=').allMatches(data.html).length, 2);
    expect(data.html, contains('&lt;test&gt; &amp; data'));
    expect(data.html, contains('<strong>12</strong>'));
    expect(data.plainText, contains('Name\tValue'));
    expect(data.plainText, contains('<test> & data\t12'));
  });

  test('rich DOCX paste preserves explicit HTML br breaks and plain text', () {
    final data = markdownRichClipboardDocument(
      '第一行<br>第二行<br/>第三行<br />第四行<BR>第五行',
    );
    expect(
      RegExp(r'<br\s*/?>', caseSensitive: false).allMatches(data.html),
      hasLength(4),
    );
    expect(data.html, isNot(contains('&lt;br')));
    expect(data.plainText, '第一行\n第二行\n第三行\n第四行\n第五行');
  });

  test('Word table copy preserves br within cells, not extra table rows', () {
    final data = markdownRichClipboardTable(const <MarkdownRichTableRow>[
      MarkdownRichTableRow(cells: ['Item', 'Details'], isHeader: true),
      MarkdownRichTableRow(
        cells: ['Row 1', 'first<br>second<br />third'],
        isHeader: false,
      ),
    ]);
    expect(RegExp(r'<tr>').allMatches(data.html), hasLength(2));
    expect(RegExp(r'<br\s*/?>').allMatches(data.html), hasLength(2));
    expect(data.html, contains('first<br />second<br />third'));
    expect(data.plainText, contains('first\nsecond\nthird'));
  });

  test('br-looking unsafe HTML and code stay literal when copied', () {
    final data = markdownRichClipboardDocument('''
Safe<br>break and **bold**

Not HTML: <br onclick="alert(1)"> and <script>run()</script>.

Inline code: \`<br>\`

\`\`\`html
first<br>second
\`\`\`
''');
    expect(data.html, contains('Safe<br />break'));
    expect(data.html, contains('&lt;br onclick'));
    expect(data.html, isNot(contains('<br onclick')));
    expect(data.html, isNot(contains('<script>')));
    expect(RegExp(r'<code[^>]*>&lt;br&gt;</code>').hasMatch(data.html), isTrue);
    expect(data.html, contains('first&lt;br&gt;second'));
    expect(data.plainText, contains('Safe\nbreak'));
    expect(data.plainText, contains('Inline code: <br>'));
    expect(data.plainText, contains('first<br>second'));
  });

  test(
    'markdown hard breaks remain actual Word and plain-text line breaks',
    () {
      final data = markdownRichClipboardDocument('line one  \nline two');
      expect(data.html, contains('<br'));
      expect(data.plainText, 'line one\nline two');
    },
  );

  test('clipboard HTML removes unsafe local and executable links', () {
    final data = markdownRichClipboardDocument('''
[safe](https://example.org) [local](../private.md) [js](javascript:alert(1))

![local image](../secret.png)
''');
    expect(data.html, contains('href="https://example.org"'));
    expect(data.html, isNot(contains('href="../private.md"')));
    expect(data.html, isNot(contains('href="javascript:')));
    expect(data.html, isNot(contains('src="../secret.png"')));
  });

  test('code fences keep literal angle-bracket content', () {
    final data = markdownRichClipboardDocument('''
```html
<script>doNotRun()</script>
```

Inline `<literal>` is code too.
''');
    expect(data.html, contains('&lt;script&gt;doNotRun()&lt;/script&gt;'));
    expect(data.html, isNot(contains('<script>')));
    expect(data.plainText, contains('<script>doNotRun()</script>'));
    expect(data.plainText, contains('<literal>'));
  });

  test(
    'rich clipboard sends HTML and text in a single native operation',
    () async {
      MethodCall? call;
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      messenger.setMockMethodCallHandler(MarkdownRichClipboard.channel, (
        value,
      ) async {
        call = value;
        return null;
      });
      addTearDown(() {
        messenger.setMockMethodCallHandler(MarkdownRichClipboard.channel, null);
      });
      final rich = markdownRichClipboardDocument('# Clipboard');
      expect(await const MarkdownRichClipboard().copy(rich), isTrue);
      expect(call?.method, 'setHtml');
      expect(call?.arguments, <String, String>{
        'html': rich.html,
        'text': rich.plainText,
      });
    },
  );
}
