part of 'workspace_git_history_surface.dart';

extension _WorkspaceGitHistoryToolbar on _WorkspaceGitHistorySurfaceState {
  Widget _buildToolbar(BuildContext context) {
    final theme = Theme.of(context);
    return SizedBox(
      height: AleraTokens.sidebarHeaderHeight,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: AleraTokens.space12),
        child: Row(
          children: <Widget>[
            const Icon(
              AleraIcons.gitGraph,
              size: 16,
              color: AleraTokens.foregroundMuted,
            ),
            const SizedBox(width: AleraTokens.space8),
            Flexible(
              child: Text(
                widget.tab.gitHistoryFilePath ?? 'Commits',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: theme.textTheme.titleSmall?.copyWith(
                  fontWeight: .w600,
                  color: AleraTokens.foreground,
                ),
              ),
            ),
            if (_items.isNotEmpty) ...<Widget>[
              const SizedBox(width: AleraTokens.space8),
              Text(
                _hasMore ? '${_items.length}+' : '${_items.length}',
                style: theme.textTheme.labelSmall?.copyWith(
                  color: AleraTokens.foregroundFaint,
                ),
              ),
            ],
            const SizedBox(width: AleraTokens.space16),
            Expanded(
              child: SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: <Widget>[
                    _buildBranchPerspectiveMenu(theme),
                    const SizedBox(width: AleraTokens.space8),
                    _buildMergedVisibilityMenu(theme),
                    if (_allBranches &&
                        _mergedBranchVisibility !=
                            WorkspaceGitHistoryMergedBranchVisibility
                                .showAll) ...<Widget>[
                      const SizedBox(width: AleraTokens.space8),
                      _buildMergedTargetMenu(theme),
                    ],
                  ],
                ),
              ),
            ),
            const SizedBox(width: AleraTokens.space8),
            if (_loading || _loadingMore)
              const Padding(
                padding: EdgeInsets.only(right: AleraTokens.space8),
                child: SizedBox(
                  width: 12,
                  height: 12,
                  child: CircularProgressIndicator(strokeWidth: 2),
                ),
              ),
            AleraIconButton(
              tooltip: 'Refresh Commits',
              icon: AleraIcons.refresh,
              onPressed: _loading
                  ? null
                  : () => unawaited(_initializeHistory()),
            ),
          ],
        ),
      ),
    );
  }
}
