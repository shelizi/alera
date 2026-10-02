part of 'workspace_git_history_surface.dart';

class const _CommitGraphRow({
  required final GitHistoryItemViewModel viewModel,
  required final double graphWidth,
  required final bool isCompareAnchor,
  required final VoidCallback onTap,
  final ValueChanged<Offset>? onOpenActions,
  final void Function(GitHistoryItemRef itemRef, Offset position)?
  onOpenRefActions,
}) extends StatelessWidget {
  static const double _refBadgeMaxWidth = 160;
  static const double _metaWidth = 88;

  @override
  Widget build(BuildContext context) {
    final item = viewModel.historyItem;
    final theme = Theme.of(context);
    final isHead = viewModel.kind == GitHistoryItemViewModelKind.head;
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: InkWell(
        onTap: onTap,
        onSecondaryTapDown: onOpenActions == null
            ? null
            : (details) => onOpenActions!(details.globalPosition),
        mouseCursor: SystemMouseCursors.click,
        child: DecoratedBox(
          key: isHead
              ? ValueKey<String>('git-history-head-row-${item.id}')
              : null,
          decoration: BoxDecoration(
            color: isCompareAnchor
                ? AleraTokens.surfaceVariant
                : isHead
                ? AleraTokens.accent.withValues(alpha: 0.08)
                : null,
            border: isHead
                ? Border(
                    left: const BorderSide(color: AleraTokens.accent, width: 3),
                    top: BorderSide(
                      color: AleraTokens.accent.withValues(alpha: 0.28),
                    ),
                    right: BorderSide(
                      color: AleraTokens.accent.withValues(alpha: 0.28),
                    ),
                    bottom: BorderSide(
                      color: AleraTokens.accent.withValues(alpha: 0.28),
                    ),
                  )
                : null,
          ),
          child: SizedBox(
            height: 28,
            child: Padding(
              padding: const EdgeInsets.only(
                left: AleraTokens.space8,
                right: AleraTokens.space12,
              ),
              child: Row(
                children: <Widget>[
                  SizedBox(
                    width: graphWidth,
                    child: Align(
                      alignment: .centerLeft,
                      child: GitHistoryGraph(viewModel: viewModel),
                    ),
                  ),
                  const SizedBox(width: AleraTokens.space4),
                  Expanded(
                    child: Row(
                      children: <Widget>[
                        for (final itemRef in item.references) ...<Widget>[
                          ConstrainedBox(
                            constraints: const BoxConstraints(
                              maxWidth: _refBadgeMaxWidth,
                            ),
                            child: GitRefBadge(
                              itemRef: itemRef,
                              isCurrent:
                                  isHead && itemRef.color == gitHistoryRefColor,
                              onOpenActions: onOpenRefActions == null
                                  ? null
                                  : (position) =>
                                        onOpenRefActions!(itemRef, position),
                            ),
                          ),
                          const SizedBox(width: AleraTokens.space4),
                        ],
                        Expanded(
                          child: Text(
                            item.subject,
                            maxLines: 1,
                            overflow: .ellipsis,
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: AleraTokens.foreground,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(width: AleraTokens.space8),
                  SizedBox(
                    width: 140,
                    child: Text(
                      item.author ?? '',
                      maxLines: 1,
                      overflow: .ellipsis,
                      style: theme.textTheme.labelSmall?.copyWith(
                        color: AleraTokens.foregroundMuted,
                      ),
                    ),
                  ),
                  const SizedBox(width: AleraTokens.space8),
                  SizedBox(
                    width: _metaWidth,
                    child: Text(
                      _WorkspaceGitHistorySurfaceState._relativeTime(
                        item.timestamp,
                      ),
                      maxLines: 1,
                      overflow: .ellipsis,
                      textAlign: .end,
                      style: theme.textTheme.labelSmall?.copyWith(
                        color: AleraTokens.foregroundFaint,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
