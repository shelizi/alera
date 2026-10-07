part of 'workspace_git_diff_surface.dart';

final class _IntralineRange {
  const _IntralineRange(this.start, this.end);

  final int start;
  final int end;
}

final class _IntralinePair {
  const _IntralinePair({required this.left, required this.right});

  final List<_IntralineRange> left;
  final List<_IntralineRange> right;
}

final class _IntralineRune {
  const _IntralineRune({
    required this.value,
    required this.start,
    required this.end,
  });

  final int value;
  final int start;
  final int end;
}

const int _maxIntralineLcsCells = 65536;

List<_IntralineRune> _intralineRunes(String text) {
  final result = <_IntralineRune>[];
  var offset = 0;
  for (final rune in text.runes) {
    final width = rune > 0xffff ? 2 : 1;
    result.add(_IntralineRune(value: rune, start: offset, end: offset + width));
    offset += width;
  }
  return result;
}

List<_IntralineRange> _rangesFromChangedRunes(
  List<_IntralineRune> runes,
  List<bool> changed,
) {
  if (runes.isEmpty) return const <_IntralineRange>[];
  final ranges = <_IntralineRange>[];
  int? start;
  for (var index = 0; index < runes.length; index += 1) {
    if (changed[index]) {
      start ??= runes[index].start;
      continue;
    }
    if (start != null) {
      ranges.add(_IntralineRange(start, runes[index - 1].end));
      start = null;
    }
  }
  if (start != null) {
    ranges.add(_IntralineRange(start, runes.last.end));
  }
  return ranges;
}

_IntralinePair _fallbackIntralineDiff(
  List<_IntralineRune> left,
  List<_IntralineRune> right,
) {
  var prefix = 0;
  while (prefix < left.length &&
      prefix < right.length &&
      left[prefix].value == right[prefix].value) {
    prefix += 1;
  }
  var leftEnd = left.length;
  var rightEnd = right.length;
  while (leftEnd > prefix &&
      rightEnd > prefix &&
      left[leftEnd - 1].value == right[rightEnd - 1].value) {
    leftEnd -= 1;
    rightEnd -= 1;
  }
  return _IntralinePair(
    left: leftEnd > prefix
        ? <_IntralineRange>[
            _IntralineRange(left[prefix].start, left[leftEnd - 1].end),
          ]
        : const <_IntralineRange>[],
    right: rightEnd > prefix
        ? <_IntralineRange>[
            _IntralineRange(right[prefix].start, right[rightEnd - 1].end),
          ]
        : const <_IntralineRange>[],
  );
}

_IntralinePair _intralineDiff(String leftText, String rightText) {
  if (leftText == rightText) {
    return const _IntralinePair(
      left: <_IntralineRange>[],
      right: <_IntralineRange>[],
    );
  }
  final left = _intralineRunes(leftText);
  final right = _intralineRunes(rightText);
  if (left.isEmpty || right.isEmpty) {
    return _IntralinePair(
      left: left.isEmpty
          ? const <_IntralineRange>[]
          : <_IntralineRange>[_IntralineRange(0, leftText.length)],
      right: right.isEmpty
          ? const <_IntralineRange>[]
          : <_IntralineRange>[_IntralineRange(0, rightText.length)],
    );
  }
  if (left.length * right.length > _maxIntralineLcsCells) {
    return _fallbackIntralineDiff(left, right);
  }

  final width = right.length + 1;
  final lcs = List<int>.filled((left.length + 1) * width, 0);
  for (var leftIndex = left.length - 1; leftIndex >= 0; leftIndex -= 1) {
    for (var rightIndex = right.length - 1; rightIndex >= 0; rightIndex -= 1) {
      final cell = leftIndex * width + rightIndex;
      if (left[leftIndex].value == right[rightIndex].value) {
        lcs[cell] = 1 + lcs[(leftIndex + 1) * width + rightIndex + 1];
      } else {
        lcs[cell] = math.max(
          lcs[(leftIndex + 1) * width + rightIndex],
          lcs[leftIndex * width + rightIndex + 1],
        );
      }
    }
  }

  final leftChanged = List<bool>.filled(left.length, true);
  final rightChanged = List<bool>.filled(right.length, true);
  var leftIndex = 0;
  var rightIndex = 0;
  while (leftIndex < left.length && rightIndex < right.length) {
    if (left[leftIndex].value == right[rightIndex].value) {
      leftChanged[leftIndex] = false;
      rightChanged[rightIndex] = false;
      leftIndex += 1;
      rightIndex += 1;
      continue;
    }
    final skipLeft = lcs[(leftIndex + 1) * width + rightIndex];
    final skipRight = lcs[leftIndex * width + rightIndex + 1];
    if (skipLeft >= skipRight) {
      leftIndex += 1;
    } else {
      rightIndex += 1;
    }
  }
  return _IntralinePair(
    left: _rangesFromChangedRunes(left, leftChanged),
    right: _rangesFromChangedRunes(right, rightChanged),
  );
}

List<_IntralineRange> _fullIntralineRange(String text) => text.isEmpty
    ? const <_IntralineRange>[]
    : <_IntralineRange>[_IntralineRange(0, text.length)];

_IntralinePair _intralinePairForSides(
  _DiffSideLine? left,
  _DiffSideLine? right,
) {
  final isDeletion = left?.kind == GitDiffLineKind.deletion;
  final isAddition = right?.kind == GitDiffLineKind.addition;
  if (isDeletion && isAddition) {
    return _intralineDiff(left!.text, right!.text);
  }
  return _IntralinePair(
    left: isDeletion
        ? _fullIntralineRange(left!.text)
        : const <_IntralineRange>[],
    right: isAddition
        ? _fullIntralineRange(right!.text)
        : const <_IntralineRange>[],
  );
}

List<List<_IntralineRange>> _buildIntralineRangesForDiffLines(
  List<GitDiffLine> lines,
) {
  final result = List<List<_IntralineRange>>.generate(
    lines.length,
    (_) => const <_IntralineRange>[],
    growable: false,
  );
  var index = 0;
  while (index < lines.length) {
    if (lines[index].kind == GitDiffLineKind.deletion) {
      final deletionStart = index;
      while (index < lines.length &&
          lines[index].kind == GitDiffLineKind.deletion) {
        index += 1;
      }
      final additionStart = index;
      while (index < lines.length &&
          lines[index].kind == GitDiffLineKind.addition) {
        index += 1;
      }
      final deletionCount = additionStart - deletionStart;
      final additionCount = index - additionStart;
      final pairCount = math.min(deletionCount, additionCount);
      for (var pairIndex = 0; pairIndex < pairCount; pairIndex += 1) {
        final deletionIndex = deletionStart + pairIndex;
        final additionIndex = additionStart + pairIndex;
        final pair = _intralineDiff(
          _extractContent(lines[deletionIndex].text),
          _extractContent(lines[additionIndex].text),
        );
        result[deletionIndex] = pair.left;
        result[additionIndex] = pair.right;
      }
      for (
        var deletionIndex = deletionStart + pairCount;
        deletionIndex < additionStart;
        deletionIndex += 1
      ) {
        result[deletionIndex] = _fullIntralineRange(
          _extractContent(lines[deletionIndex].text),
        );
      }
      for (
        var additionIndex = additionStart + pairCount;
        additionIndex < index;
        additionIndex += 1
      ) {
        result[additionIndex] = _fullIntralineRange(
          _extractContent(lines[additionIndex].text),
        );
      }
      continue;
    }
    if (lines[index].kind == GitDiffLineKind.addition) {
      result[index] = _fullIntralineRange(_extractContent(lines[index].text));
    }
    index += 1;
  }
  return result;
}

TextSpan _intralineHighlightedSpan({
  required String text,
  required TextSpan baseSpan,
  required List<_IntralineRange> ranges,
  required Color background,
}) {
  if (ranges.isEmpty || text.isEmpty) return baseSpan;
  final leaves = <({String text, TextStyle? style})>[];
  var unsupportedChild = false;

  void flatten(TextSpan span, TextStyle? inherited) {
    final style = inherited?.merge(span.style) ?? span.style;
    final ownText = span.text;
    if (ownText != null && ownText.isNotEmpty) {
      leaves.add((text: ownText, style: style));
    }
    for (final child in span.children ?? const <InlineSpan>[]) {
      if (child is TextSpan) {
        flatten(child, style);
      } else {
        unsupportedChild = true;
      }
    }
  }

  flatten(baseSpan, null);
  if (unsupportedChild || leaves.map((leaf) => leaf.text).join() != text) {
    return baseSpan;
  }

  final children = <InlineSpan>[];
  var offset = 0;
  for (final leaf in leaves) {
    final leafStart = offset;
    final leafEnd = offset + leaf.text.length;
    final cuts = <int>{leafStart, leafEnd};
    for (final range in ranges) {
      if (range.end <= leafStart || range.start >= leafEnd) continue;
      cuts.add(range.start.clamp(leafStart, leafEnd));
      cuts.add(range.end.clamp(leafStart, leafEnd));
    }
    final sortedCuts = cuts.toList()..sort();
    for (var cutIndex = 0; cutIndex + 1 < sortedCuts.length; cutIndex += 1) {
      final start = sortedCuts[cutIndex];
      final end = sortedCuts[cutIndex + 1];
      if (end <= start) continue;
      final highlighted = ranges.any(
        (range) => range.start < end && range.end > start,
      );
      children.add(
        TextSpan(
          text: text.substring(start, end),
          style: highlighted
              ? (leaf.style ?? const TextStyle()).copyWith(
                  backgroundColor: background,
                )
              : leaf.style,
        ),
      );
    }
    offset = leafEnd;
  }
  return TextSpan(children: children);
}
