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
                    TextSpan(
                      text: '${line.text}\n',
                      style: TextStyle(
                        backgroundColor: switch (line.kind) {
                          MobileGitPatchLineKind.added => Theme.of(
                            context,
                          ).colorScheme.tertiary.withValues(alpha: 0.16),
                          MobileGitPatchLineKind.removed => Theme.of(
                            context,
                          ).colorScheme.error.withValues(alpha: 0.15),
                          MobileGitPatchLineKind.hunk => Theme.of(
                            context,
                          ).colorScheme.primary.withValues(alpha: 0.12),
                          _ => null,
                        },
                        fontWeight: switch (line.kind) {
                          MobileGitPatchLineKind.header ||
                          MobileGitPatchLineKind.hunk => FontWeight.w600,
                          _ => FontWeight.normal,
                        },
                      ),
                    ),
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
