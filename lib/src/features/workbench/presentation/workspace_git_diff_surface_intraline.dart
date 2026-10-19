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

// Keep the whole-line tint subtle while making the characters that actually
// changed easy to distinguish against Alera's dark surface.
const double _diffIntralineHighlightAlpha = 0.45;

Color _diffIntralineHighlightColor(GitDiffLineKind kind) => switch (kind) {
  GitDiffLineKind.addition => AleraTokens.success.withValues(
    alpha: _diffIntralineHighlightAlpha,
  ),
  GitDiffLineKind.deletion => AleraTokens.error.withValues(
    alpha: _diffIntralineHighlightAlpha,
  ),
  _ => Colors.transparent,
};

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
    if (left!.nativeRanges != null && right!.nativeRanges != null) {
      return _IntralinePair(
        left: left.nativeRanges!,
        right: right.nativeRanges!,
      );
    }
    return _intralineDiff(left.text, right!.text);
  }
  return _IntralinePair(
    left: isDeletion
        ? (left!.nativeRanges ?? _fullIntralineRange(left.text))
        : const <_IntralineRange>[],
    right: isAddition
        ? (right!.nativeRanges ?? _fullIntralineRange(right.text))
        : const <_IntralineRange>[],
  );
}

const int _maxDiffLineAlignmentCells = 4096;

int _diffLineMatchScore(String left, String right) {
  if (left == right) return 2000;
  final a = left.trim().runes.toList(growable: false);
  final b = right.trim().runes.toList(growable: false);
  if (a.isEmpty || b.isEmpty) return 0;
  var prefix = 0;
  while (prefix < a.length && prefix < b.length && a[prefix] == b[prefix]) {
    prefix++;
  }
  var suffix = 0;
  while (suffix < a.length - prefix &&
      suffix < b.length - prefix &&
      a[a.length - suffix - 1] == b[b.length - suffix - 1]) {
    suffix++;
  }
  final anchored = prefix + suffix;
  var common = anchored;
  if (a.length * b.length <= 4096) {
    final below = List<int>.filled(b.length + 1, 0);
    final row = List<int>.filled(b.length + 1, 0);
    for (var i = a.length - 1; i >= 0; i--) {
      row[b.length] = 0;
      for (var j = b.length - 1; j >= 0; j--) {
        row[j] = a[i] == b[j]
            ? 1 + below[j + 1]
            : math.max(below[j], row[j + 1]);
      }
      for (var j = 0; j <= b.length; j++) {
        below[j] = row[j];
      }
    }
    common = below[0];
  }
  final score = 200 * (7 * common + 3 * anchored) ~/ (a.length + b.length);
  return score >= 480 ? score : 0;
}

List<(int?, int?)> _alignDiffChangedLines(
  List<String> oldLines,
  List<String> newLines,
) {
  final height = oldLines.length;
  final width = newLines.length;
  if (height * width > _maxDiffLineAlignmentCells) {
    final positions = <String, List<int>>{};
    for (var j = 0; j < width; j++) {
      positions.putIfAbsent(newLines[j], () => <int>[]).add(j);
    }
    final result = <(int?, int?)>[];
    var next = 0;
    for (var i = 0; i < height; i++) {
      final candidates = positions[oldLines[i]] ?? const <int>[];
      int? match;
      for (final candidate in candidates) {
        if (candidate >= next) {
          match = candidate;
          break;
        }
      }
      if (match == null) {
        result.add((i, null));
      } else {
        for (var j = next; j < match; j++) {
          result.add((null, j));
        }
        result.add((i, match));
        next = match + 1;
      }
    }
    for (var j = next; j < width; j++) {
      result.add((null, j));
    }
    return result;
  }

  final stride = width + 1;
  final matches = List<int>.filled(height * width, 0);
  final best = List<int>.filled((height + 1) * stride, 0);
  for (var i = height - 1; i >= 0; i--) {
    for (var j = width - 1; j >= 0; j--) {
      final score = _diffLineMatchScore(oldLines[i], newLines[j]);
      matches[i * width + j] = score;
      best[i * stride + j] = math.max(
        math.max(best[(i + 1) * stride + j], best[i * stride + j + 1]),
        score > 0 ? score + best[(i + 1) * stride + j + 1] : 0,
      );
    }
  }
  final result = <(int?, int?)>[];
  var i = 0;
  var j = 0;
  while (i < height || j < width) {
    if (i == height) {
      result.add((null, j++));
    } else if (j == width) {
      result.add((i++, null));
    } else {
      final score = matches[i * width + j];
      final skipOld = best[(i + 1) * stride + j];
      final skipNew = best[i * stride + j + 1];
      if (score > 0 &&
          score + best[(i + 1) * stride + j + 1] >=
              math.max(skipOld, skipNew)) {
        result.add((i++, j++));
      } else if (skipOld >= skipNew) {
        result.add((i++, null));
      } else {
        result.add((null, j++));
      }
    }
  }
  return result;
}

List<List<_IntralineRange>> _buildIntralineRangesForDiffLines(
  List<GitDiffLine> lines, {
  List<List<GitIntralineRange>>? nativeRanges,
}) {
  if (nativeRanges != null && nativeRanges.length == lines.length) {
    return nativeRanges
        .map(
          (ranges) => ranges
              .map((range) => _IntralineRange(range.start, range.end))
              .toList(growable: false),
        )
        .toList(growable: false);
  }
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
      final deletions = [
        for (var i = deletionStart; i < additionStart; i++)
          _extractContent(lines[i].text),
      ];
      final additions = [
        for (var i = additionStart; i < index; i++)
          _extractContent(lines[i].text),
      ];
      for (final (left, right) in _alignDiffChangedLines(
        deletions,
        additions,
      )) {
        if (left != null && right != null) {
          final pair = _intralineDiff(deletions[left], additions[right]);
          result[deletionStart + left] = pair.left;
          result[additionStart + right] = pair.right;
        } else if (left != null) {
          result[deletionStart + left] = _fullIntralineRange(deletions[left]);
        } else if (right != null) {
          result[additionStart + right] = _fullIntralineRange(additions[right]);
        }
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
