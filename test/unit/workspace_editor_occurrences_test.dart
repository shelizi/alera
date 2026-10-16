import 'package:alera/src/features/workbench/presentation/workspace_editor_surface.dart';
import 'package:code_forge/code_forge.dart' as code_forge;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('only identical case-sensitive, whole identifiers are highlighted', () {
    final doc = _Lines([
      'token tokenized token_token',
      'token + token',
      'TOKEN token',
    ]);
    expect(
      _positions(
        doc.occurrences(const TextSelection(baseOffset: 0, extentOffset: 5)),
      ),
      ['1:0-5', '1:8-13', '2:6-11'],
    );
  });

  test('selected substring inside an identifier does not highlight', () {
    final doc = _Lines(['prefixVariable variable variableSuffix']);
    expect(
      doc.occurrences(const TextSelection(baseOffset: 6, extentOffset: 14)),
      isEmpty,
    );
  });

  test('selected expressions and phrases match as literal strings', () {
    final doc = _Lines(['foo.bar + foo.bar fooXbar', 'foo.bar']);
    expect(
      _positions(
        doc.occurrences(const TextSelection(baseOffset: 0, extentOffset: 7)),
      ),
      ['0:10-17', '1:0-7'],
    );
  });

  test('handles dollar signs, underscores, and Unicode identifier names', () {
    final dollar = _Lines([r'$var $_var $var var']);
    expect(
      _positions(
        dollar.occurrences(const TextSelection(baseOffset: 0, extentOffset: 4)),
      ),
      ['0:11-15'],
    );

    final unicode = _Lines(['😀 名稱 名稱', '名稱😀 名稱']);
    final matches = unicode.occurrences(
      const TextSelection(baseOffset: 2, extentOffset: 4),
    );
    expect(_positions(matches), ['0:5-7', '1:0-2', '1:4-6']);
  });

  test(
    'ignores collapsed, multiline, punctuation and overly long selections',
    () {
      final doc = _Lines(['one + two', 'one + two']);
      expect(
        doc.occurrences(const TextSelection.collapsed(offset: 2)),
        isEmpty,
      );
      expect(
        doc.occurrences(const TextSelection(baseOffset: 0, extentOffset: 13)),
        isEmpty,
      );
      expect(
        doc.occurrences(const TextSelection(baseOffset: 4, extentOffset: 5)),
        isEmpty,
      );
      final longIdentifier = _Lines(['${'x' * 65} ${'x' * 65}']);
      expect(
        longIdentifier.occurrences(
          const TextSelection(baseOffset: 0, extentOffset: 65),
        ),
        isEmpty,
      );
    },
  );

  test('caps occurrences and never highlights active selection', () {
    final doc = _Lines(['item item item item item']);
    expect(
      _positions(
        doc.occurrences(
          const TextSelection(baseOffset: 5, extentOffset: 9),
          maxMatches: 2,
        ),
      ),
      ['0:0-4', '0:10-14'],
    );
  });

  test('large files scan only nearby lines, not the full document', () {
    final doc = _Lines(List.filled(6000, 'field'));
    final selectedStart = doc.startOffsets[3000];
    final matches = doc.occurrences(
      TextSelection(baseOffset: selectedStart, extentOffset: selectedStart + 5),
      largeFileMode: true,
    );
    expect(matches.length, 320);
    expect(matches.first.startLine, 2840);
    expect(matches.last.startLine, 3160);
    expect(matches.any((match) => match.startLine == 3000), isFalse);
    expect(doc.readLines, lessThanOrEqualTo(322));
  });

  test('skips extremely long lines instead of scanning them', () {
    final doc = _Lines(['foo', 'foo' * 100000, 'foo']);
    expect(
      _positions(
        doc.occurrences(
          const TextSelection(baseOffset: 0, extentOffset: 3),
          largeFileMode: true,
        ),
      ),
      ['2:0-3'],
    );
  });
}

class _Lines {
  _Lines(this.lines) {
    var offset = 0;
    for (final line in lines) {
      startOffsets.add(offset);
      offset += line.runes.length + 1;
    }
  }

  final List<String> lines;
  final List<int> startOffsets = [];
  int readLines = 0;

  List<code_forge.DocumentHighlight> occurrences(
    TextSelection selection, {
    bool largeFileMode = false,
    int maxMatches = 400,
  }) => workspaceEditorSelectedTextOccurrences(
    selection: selection,
    lineCount: lines.length,
    documentLength: startOffsets.last + lines.last.runes.length,
    lineForOffset: (offset) {
      for (var i = startOffsets.length - 1; i >= 0; i--) {
        if (offset >= startOffsets[i]) return i;
      }
      return 0;
    },
    lineStartOffset: (line) => startOffsets[line],
    lineText: (line) {
      readLines++;
      return lines[line];
    },
    largeFileMode: largeFileMode,
    maxMatches: maxMatches,
  );
}

List<String> _positions(List<code_forge.DocumentHighlight> highlights) => [
  for (final highlight in highlights)
    '${highlight.startLine}:${highlight.startColumn}-${highlight.endColumn}',
];
