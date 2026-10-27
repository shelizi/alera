part of 'workspace_git_history_surface.dart';

extension _WorkspaceGitHistoryToolbar on _WorkspaceGitHistorySurfaceState {
  Widget _buildToolbar(BuildContext context) {
    final theme = Theme.of(context);
    return SizedBox(
      height: AleraTokens.sidebarHeaderHeight,
      child: LayoutBuilder(
        builder: (context, constraints) {
          // Leave room for the separately anchored refresh button. On narrow
          // windows, scroll the grouped actions from their trailing edge so
          // Compare Branches stays visible and pinned to the right.
          final contentWidth = math.max(
            0.0,
            constraints.maxWidth - AleraTokens.space12 - 50,
          );
          final compactActions = constraints.maxWidth < 700;
          return Stack(
            children: <Widget>[
              Padding(
                padding: const EdgeInsets.only(
                  left: AleraTokens.space12,
                  right: 50,
                ),
                child: Row(
                  children: <Widget>[
                    const Icon(
                      AleraIcons.gitGraph,
                      size: 16,
                      color: AleraTokens.foregroundMuted,
                    ),
                    const SizedBox(width: AleraTokens.space8),
                    ConstrainedBox(
                      constraints: BoxConstraints(
                        maxWidth: math.min(220.0, contentWidth * 0.24),
                      ),
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
                    const SizedBox(width: AleraTokens.space12),
                    Expanded(
                      child: SingleChildScrollView(
                        scrollDirection: Axis.horizontal,
                        child: _buildBranchPerspectiveMenu(theme),
                      ),
                    ),
                    const SizedBox(width: AleraTokens.space8),
                    ConstrainedBox(
                      constraints: BoxConstraints(
                        maxWidth: contentWidth * 0.62,
                      ),
                      child: SingleChildScrollView(
                        scrollDirection: Axis.horizontal,
                        reverse: true,
                        child: Row(
                          key: const ValueKey<String>(
                            'git-history-right-branch-actions',
                          ),
                          mainAxisSize: MainAxisSize.min,
                          children: <Widget>[
                            _buildMergedVisibilityMenu(theme),
                            if (_allBranches &&
                                _mergedBranchVisibility !=
                                    WorkspaceGitHistoryMergedBranchVisibility
                                        .showAll) ...<Widget>[
                              const SizedBox(width: AleraTokens.space8),
                              _buildMergedTargetMenu(theme),
                            ],
                            const SizedBox(width: AleraTokens.space8),
                            if (compactActions)
                              Tooltip(
                                message: 'Compare Branches',
                                child: OutlinedButton(
                                  key: const ValueKey<String>(
                                    'git-history-compare-branches',
                                  ),
                                  onPressed: _loading
                                      ? null
                                      : () => unawaited(_openBranchCompare()),
                                  style: OutlinedButton.styleFrom(
                                    minimumSize: const Size(36, 32),
                                    padding: const EdgeInsets.symmetric(
                                      horizontal: AleraTokens.space8,
                                    ),
                                  ),
                                  child: const Icon(
                                    Icons.compare_arrows,
                                    size: 16,
                                  ),
                                ),
                              )
                            else
                              OutlinedButton.icon(
                                key: const ValueKey<String>(
                                  'git-history-compare-branches',
                                ),
                                onPressed: _loading
                                    ? null
                                    : () => unawaited(_openBranchCompare()),
                                icon: const Icon(
                                  Icons.compare_arrows,
                                  size: 16,
                                ),
                                label: const Text('Compare Branches'),
                              ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
              if (_loading || _loadingMore)
                const Positioned(
                  right: 50,
                  top: 16,
                  child: SizedBox(
                    width: 12,
                    height: 12,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
                ),
              Positioned(
                top: 11,
                right: AleraTokens.space12,
                child: AleraIconButton(
                  tooltip: 'Refresh Commits',
                  icon: AleraIcons.refresh,
                  onPressed: _loading
                      ? null
                      : () => unawaited(_initializeHistory()),
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}
