import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_segmented_button.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter/material.dart';

class const WorkspaceGitBranchCompareStatusFilter({
  super.key,
  required final GitChangeStatus? selected,
  required final ValueChanged<GitChangeStatus?> onChanged,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(
      AleraTokens.space8,
      0,
      AleraTokens.space8,
      AleraTokens.space8,
    ),
    child: AleraSegmentedButton<GitChangeStatus?>(
      key: const ValueKey('branch-compare-status-filter'),
      selected: selected,
      onSelectionChanged: onChanged,
      dense: true,
      padding: const EdgeInsets.symmetric(
        horizontal: AleraTokens.space6,
        vertical: AleraTokens.space4,
      ),
      backgroundColor: AleraTokens.surface,
      foregroundColor: AleraTokens.foregroundMuted,
      selectedBackgroundColor: AleraTokens.surfaceElevated,
      selectedForegroundColor: AleraTokens.foreground,
      borderColor: AleraTokens.borderSubtle,
      textStyle: Theme.of(context).textTheme.labelSmall,
      segments: const [
        ButtonSegment(
          value: null,
          label: Text('All'),
          tooltip: 'All Changed Files',
        ),
        ButtonSegment(
          value: GitChangeStatus.added,
          label: Text('A'),
          tooltip: 'Added Files',
        ),
        ButtonSegment(
          value: GitChangeStatus.modified,
          label: Text('M'),
          tooltip: 'Modified Files',
        ),
        ButtonSegment(
          value: GitChangeStatus.deleted,
          label: Text('D'),
          tooltip: 'Deleted Files',
        ),
        ButtonSegment(
          value: GitChangeStatus.renamed,
          label: Text('R'),
          tooltip: 'Renamed Files',
        ),
        ButtonSegment(
          value: GitChangeStatus.copied,
          label: Text('C'),
          tooltip: 'Copied Files',
        ),
      ],
    ),
  );
}
