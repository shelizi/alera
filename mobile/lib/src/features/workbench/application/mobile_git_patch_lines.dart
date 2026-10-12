import 'dart:convert';

enum MobileGitPatchLineKind { header, hunk, added, removed, context }

class const MobileGitPatchLine({
  required final String text,
  required final MobileGitPatchLineKind kind,
  final int? changedStart,
  final int? changedEnd,
});

/// Keep the original line markers for accurate copy/paste. File headers
/// `+++` and `---` are not actual added/removed source lines.
List<MobileGitPatchLine> classifyMobileGitPatchLines(String patch) {
  final lines = <MobileGitPatchLine>[
    for (final line in const LineSplitter().convert(patch))
      MobileGitPatchLine(
        text: line,
        kind: switch (line) {
          final String value when value.startsWith('@@') =>
            MobileGitPatchLineKind.hunk,
          final String value
              when value.startsWith('diff --git ') ||
                  value.startsWith('index ') ||
                  value.startsWith('--- ') ||
                  value.startsWith('+++ ') =>
            MobileGitPatchLineKind.header,
          final String value when value.startsWith('+') =>
            MobileGitPatchLineKind.added,
          final String value when value.startsWith('-') =>
            MobileGitPatchLineKind.removed,
          _ => MobileGitPatchLineKind.context,
        },
      ),
  ];
  // Only equal-size, small contiguous -/+ groups have unambiguous line pairs.
  // Unequal groups, moves and long lines retain the existing whole-line colors.
  for (var index = 0; index < lines.length;) {
    if (lines[index].kind != MobileGitPatchLineKind.removed) {
      index++;
      continue;
    }
    var firstAdded = index;
    while (firstAdded < lines.length &&
        lines[firstAdded].kind == MobileGitPatchLineKind.removed) {
      firstAdded++;
    }
    var end = firstAdded;
    while (end < lines.length &&
        lines[end].kind == MobileGitPatchLineKind.added) {
      end++;
    }
    final count = firstAdded - index;
    if (count == end - firstAdded && count <= 8) {
      for (var pair = 0; pair < count; pair++) {
        final removed = lines[index + pair];
        final added = lines[firstAdded + pair];
        final bounds = _changedUnicodeBounds(removed.text, added.text);
        if (bounds == null) continue;
        lines[index + pair] = MobileGitPatchLine(
          text: removed.text,
          kind: removed.kind,
          changedStart: bounds.$1,
          changedEnd: bounds.$2,
        );
        lines[firstAdded + pair] = MobileGitPatchLine(
          text: added.text,
          kind: added.kind,
          changedStart: bounds.$3,
          changedEnd: bounds.$4,
        );
      }
    }
    index = end;
  }
  return lines;
}

/// Indices include the +/- prefix (always one UTF-16 code unit) and never
/// split Unicode surrogate pairs. Each input line has a hard work limit.
(int, int, int, int)? _changedUnicodeBounds(String oldLine, String newLine) {
  if (oldLine.length > 4096 || newLine.length > 4096) return null;
  final oldRunes = oldLine.substring(1).runes.toList(growable: false);
  final newRunes = newLine.substring(1).runes.toList(growable: false);
  var commonPrefix = 0;
  while (commonPrefix < oldRunes.length &&
      commonPrefix < newRunes.length &&
      oldRunes[commonPrefix] == newRunes[commonPrefix]) {
    commonPrefix++;
  }
  if (commonPrefix == oldRunes.length && commonPrefix == newRunes.length) {
    return null;
  }
  var commonSuffix = 0;
  while (commonSuffix < oldRunes.length - commonPrefix &&
      commonSuffix < newRunes.length - commonPrefix &&
      oldRunes[oldRunes.length - 1 - commonSuffix] ==
          newRunes[newRunes.length - 1 - commonSuffix]) {
    commonSuffix++;
  }
  final oldStart = 1 + String.fromCharCodes(oldRunes.take(commonPrefix)).length;
  final newStart = 1 + String.fromCharCodes(newRunes.take(commonPrefix)).length;
  final oldEnd =
      oldLine.length -
      String.fromCharCodes(oldRunes.skip(oldRunes.length - commonSuffix))
          .length;
  final newEnd =
      newLine.length -
      String.fromCharCodes(newRunes.skip(newRunes.length - commonSuffix))
          .length;
  return (oldStart, oldEnd, newStart, newEnd);
}
