import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_git_patch_lines.dart';
import 'package:flutter/material.dart';

/// Shared selectable, line-highlighted and bounded Git patch viewer.
class MobileGitPatchView extends StatelessWidget {
  const MobileGitPatchView({
    super.key,
    required this.patch,
    required this.truncated,
    this.emptyMessage = 'No printable patch for this file.',
  });

  final String patch;
  final bool truncated;
  final String emptyMessage;

  TextSpan _renderLine(BuildContext context, MobileGitPatchLine line) {
    final colors = Theme.of(context).colorScheme;
    final lineStyle = TextStyle(
      backgroundColor: switch (line.kind) {
        MobileGitPatchLineKind.added => colors.tertiary.withValues(alpha: 0.16),
        MobileGitPatchLineKind.removed => colors.error.withValues(alpha: 0.15),
        MobileGitPatchLineKind.hunk => colors.primary.withValues(alpha: 0.12),
        _ => null,
      },
      fontWeight: switch (line.kind) {
        MobileGitPatchLineKind.header ||
        MobileGitPatchLineKind.hunk => FontWeight.w600,
        _ => FontWeight.normal,
      },
    );
    final start = line.changedStart;
    final end = line.changedEnd;
    if (start == null || end == null || start >= end) {
      return TextSpan(text: '${line.text}\n', style: lineStyle);
    }
    final emphasisColor = line.kind == MobileGitPatchLineKind.added
        ? colors.tertiary
        : colors.error;
    return TextSpan(
      style: lineStyle,
      children: <TextSpan>[
        TextSpan(text: line.text.substring(0, start)),
        TextSpan(
          text: line.text.substring(start, end),
          style: TextStyle(
            backgroundColor: emphasisColor.withValues(alpha: 0.34),
            fontWeight: FontWeight.w700,
          ),
        ),
        TextSpan(text: '${line.text.substring(end)}\n'),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    if (patch.isEmpty) {
      return Center(child: Text(emptyMessage));
    }
    return Column(
      children: <Widget>[
        if (truncated)
          const Padding(
            padding: AleraTokens.contentPadding,
            child: Text('Patch truncated at 512 KiB.'),
          ),
        Expanded(
          child: SingleChildScrollView(
            padding: AleraTokens.contentPadding,
            child: SelectableText.rich(
              TextSpan(
                children: <TextSpan>[
                  for (final line in classifyMobileGitPatchLines(patch))
                    _renderLine(context, line),
                ],
              ),
              style: Theme.of(context).textTheme.bodySmall
                  ?.copyWith(fontFamily: AleraTokens.monoFontFamily),
            ),
          ),
        ),
      ],
    );
  }
}
