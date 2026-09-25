import 'package:code_forge/code_forge.dart';
import 'package:code_forge/code_forge/syntax_highlighter.dart';
import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:re_highlight/re_highlight.dart';

const _semanticColor = Color(0xFF00FF00);

void main() {
  SyntaxHighlighter highlighter() => SyntaxHighlighter(
    language: Mode(),
    languageId: 'dart',
    editorTheme: const <String, TextStyle>{
      'root': TextStyle(color: Color(0xFFFFFFFF)),
      'section': TextStyle(color: _semanticColor),
    },
  );

  LspSemanticToken function(int line, int start, int length) =>
      LspSemanticToken(
        line: line,
        start: start,
        length: length,
        typeIndex: 0,
        modifierBitmask: 0,
        tokenTypeName: 'function',
      );

  void seed(SyntaxHighlighter highlighter, List<String> lines) {
    highlighter.updateSemanticTokens(
      [function(lines.length - 1, 4, 3)],
      (line) => lines[line],
      lines.length,
    );
  }

  test('keeps the span on its word when text is inserted before it', () {
    final lines = ['first line', 'var foo = 1;'];
    final subject = highlighter();
    seed(subject, lines);

    subject.applyDocumentEdit(1, 0, 0, 'xx', '', '');

    expect(subject.getLineSpanWords(1, 'xxvar foo = 1;'), ['foo']);
  });

  test('moves the tail of a split line to the new line', () {
    final lines = ['first line', 'var foo = 1;'];
    final subject = highlighter();
    seed(subject, lines);

    subject.applyDocumentEdit(1, 3, 3, '\n', '', '');

    expect(subject.getLineSpanWords(1, 'var'), isEmpty);
    expect(subject.getLineSpanWords(2, ' foo = 1;'), ['foo']);
  });

  test('drops stale spans on covered lines that received no token', () {
    final lines = ['first line', 'var foo = 1;'];
    final subject = highlighter();
    seed(subject, lines);

    subject.updateSemanticTokens(
      const [],
      (line) => lines[line],
      lines.length,
      coveredLines: (startLine: 0, endLine: 1),
    );

    expect(subject.getLineSpanWords(1, lines[1]), isEmpty);
  });

  test('keeps spans outside the covered lines', () {
    final lines = ['first line', 'var foo = 1;'];
    final subject = highlighter();
    seed(subject, lines);

    subject.updateSemanticTokens(
      const [],
      (line) => lines[line],
      lines.length,
      coveredLines: (startLine: 0, endLine: 0),
    );

    expect(subject.getLineSpanWords(1, lines[1]), ['foo']);
  });

  test('places a span after an astral character on its word', () {
    // LSP reports UTF-16 columns: the emoji takes two, so `foo` starts at 5.
    const line = '😀 = foo;';
    final subject = highlighter();
    subject.updateSemanticTokens([function(0, 5, 3)], (_) => line, 1);

    expect(subject.getLineSpanWords(0, line), ['foo']);
  });
}

extension on SyntaxHighlighter {
  List<String> getLineSpanWords(int lineIndex, String line) {
    final span = getLineSpan(lineIndex, line);
    final words = <String>[];
    void visit(InlineSpan node) {
      if (node is TextSpan) {
        if (node.style?.color == _semanticColor && node.text != null) {
          words.add(node.text!);
        }
        node.children?.forEach(visit);
      }
    }

    if (span != null) visit(span);
    return words;
  }
}
