import 'dart:convert';

enum MobileGitPatchLineKind { header, hunk, added, removed, context }

class const MobileGitPatchLine({
  required final String text,
  required final MobileGitPatchLineKind kind,
});

/// Keep the original line markers for accurate copy/paste. File headers
/// `+++` and `---` are not actual added/removed source lines.
List<MobileGitPatchLine> classifyMobileGitPatchLines(String patch) =>
    <MobileGitPatchLine>[
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
