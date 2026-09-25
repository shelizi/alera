import 'package:code_forge/code_forge.dart' as code_forge;
import 'package:flutter_test/flutter_test.dart';

void main() {
  ({bool closed, List<(int, int)> ranges, List<int> readLines}) scan(
    List<String> lines,
    int startLine,
  ) {
    final readLines = <int>[];
    final ranges = <(int, int)>[];
    final closed = code_forge.codeForgeScanBracketFolds(
      startLine: startLine,
      endLineExclusive: lines.length,
      lineAt: (line) {
        readLines.add(line);
        return lines[line];
      },
      onRange: (start, end) => ranges.add((start, end)),
    );
    return (closed: closed, ranges: ranges, readLines: readLines);
  }

  final tail = List.filled(5000, '    x = call(a, b)');

  test('stops after the start line when its brackets close in place', () {
    final result = scan(['def f(a, b):', ...tail], 0);

    expect(result.closed, isFalse);
    expect(result.ranges, isEmpty);
    expect(result.readLines, [0]);
  });

  test('stops at the line that closes a multi-line bracket', () {
    final result = scan([
      'items = [',
      '    call(1),',
      '    2,',
      ']',
      ...tail,
    ], 0);

    expect(result.closed, isTrue);
    expect(result.ranges, [(0, 3)]);
    expect(result.readLines, [0, 1, 2, 3]);
  });

  test('reports nested multi-line pairs before the outer one closes', () {
    final result = scan([
      'outer = {',
      '    "a": (',
      '        1,',
      '    ),',
      '}',
    ], 0);

    expect(result.closed, isTrue);
    expect(result.ranges, [(1, 3), (0, 4)]);
  });

  test('keeps scanning to the end while a start bracket stays open', () {
    final result = scan(['call(', '    1,', '    2'], 0);

    expect(result.closed, isFalse);
    expect(result.ranges, isEmpty);
    expect(result.readLines, [0, 1, 2]);
  });

  test('treats only printable ASCII as fixed-column text', () {
    expect(code_forge.codeForgeIsPrintableAscii(''), isTrue);
    expect(code_forge.codeForgeIsPrintableAscii('def f(a): ~'), isTrue);
    expect(code_forge.codeForgeIsPrintableAscii('\tindent'), isFalse);
    expect(code_forge.codeForgeIsPrintableAscii('name = "中文"'), isFalse);
    expect(code_forge.codeForgeIsPrintableAscii('café'), isFalse);
  });

  test('ignores closers without a matching opener', () {
    final result = scan([') + wrap(', '    1,', ')'], 0);

    expect(result.closed, isTrue);
    expect(result.ranges, [(0, 2)]);
  });
}
