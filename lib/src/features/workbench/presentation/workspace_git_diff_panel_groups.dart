part of 'workspace_git_diff_panel.dart';

class const _GitDiffGroups({
  required final List<GitChangeGroup> groups,
  required final String workspacePath,
  required final GitDiffViewMode viewMode,
  required final bool busy,
  required final Set<String> collapsedSections,
  required final Set<String> collapsedTreeNodes,
  required final Set<String> expandedSubmodules,
  required final ValueChanged<String> onToggleSection,
  required final ValueChanged<String> onToggleTreeNode,
  required final ValueChanged<GitChangeEntry> onToggleSubmodule,
  required final OpenGitDiffTabCallback onOpenGitDiff,
  final ValueChanged<String>? onOpenFile,
  final ValueChanged<String>? onOpenInZed,
  required final ValueChanged<String> onRevealInExplorer,
  required final ValueChanged<GitChangeEntry> onStage,
  required final ValueChanged<GitChangeEntry> onUnstage,
  required final ValueChanged<GitChangeEntry> onDiscard,
  required final void Function(GitChangeArea area, String? filePath)
  onStageArea,
  required final void Function(GitChangeArea area, String? filePath)
  onUnstageArea,
  required final void Function(GitChangeArea area, String? filePath)
  onDiscardArea,
  required final ValueChanged<String?> onStagePath,
  required final ValueChanged<String?> onUnstagePath,
  required final ValueChanged<String?> onDiscardPath,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    // Rows must stay lazy: eagerly inflating thousands of entries is what
    // stalls the desktop UI when a workspace reports a large change set.
    return CustomScrollView(
      slivers: <Widget>[
        SliverPadding(
          padding: const EdgeInsets.symmetric(vertical: AleraTokens.space6),
          sliver: SliverMainAxisGroup(
            slivers: <Widget>[
              for (final group in groups) ..._sliversForGroup(group),
            ],
          ),
        ),
      ],
    );
  }

  List<Widget> _sliversForGroup(GitChangeGroup group) {
    if (group.entries.isEmpty) {
      return const <Widget>[];
    }
    final collapsed = collapsedSections.contains(_sectionKeyForGroup(group));
    return <Widget>[
      SliverPadding(
        padding: const EdgeInsets.only(bottom: AleraTokens.space8),
        sliver: SliverMainAxisGroup(
          slivers: <Widget>[
            SliverToBoxAdapter(
              child: _GitDiffGroupHeader(
                group: group,
                collapsed: collapsed,
                busy: busy,
                onToggleCollapsed: () =>
                    onToggleSection(_sectionKeyForGroup(group)),
                onStage: () => group.unified
                    ? onStagePath(null)
                    : onStageArea(group.area, null),
                onUnstage: () => group.unified
                    ? onUnstagePath(null)
                    : onUnstageArea(group.area, null),
                onDiscard: () => group.unified
                    ? onDiscardPath(null)
                    : onDiscardArea(group.area, null),
                canStage: group.entries.any(
                  (entry) => entry.canStageFromParent,
                ),
                canUnstage: group.entries.any(
                  (entry) => entry.canUnstageFromParent,
                ),
                canDiscard: group.entries.any(
                  (entry) => entry.canDiscardFromParent,
                ),
              ),
            ),
            if (!collapsed)
              viewMode == GitDiffViewMode.flat
                  ? SliverList.builder(
                      itemCount: group.entries.length,
                      itemBuilder: (context, index) =>
                          _buildFlatEntry(group, group.entries[index]),
                    )
                  : _GitDiffTree(
                      workspacePath: workspacePath,
                      area: group.area,
                      rows: group.treeRows,
                      busy: busy,
                      collapsedTreeNodes: collapsedTreeNodes,
                      expandedSubmodules: expandedSubmodules,
                      onToggleTreeNode: onToggleTreeNode,
                      onToggleSubmodule: onToggleSubmodule,
                      onOpenGitDiff: onOpenGitDiff,
                      onOpenFile: onOpenFile,
                      onOpenInZed: onOpenInZed,
                      onRevealInExplorer: onRevealInExplorer,
                      onStage: onStage,
                      onUnstage: onUnstage,
                      onDiscard: onDiscard,
                      onStageArea: onStageArea,
                      onUnstageArea: onUnstageArea,
                      onDiscardArea: onDiscardArea,
                      showAreaMarker: group.unified,
                      unified: group.unified,
                      onStagePath: onStagePath,
                      onUnstagePath: onUnstagePath,
                      onDiscardPath: onDiscardPath,
                    ),
          ],
        ),
      ),
    ];
  }

  Widget _buildFlatEntry(GitChangeGroup group, GitChangeEntry entry) {
    final submoduleExpanded = expandedSubmodules.contains(entry.id);
    final children = <Widget>[
      _GitDiffFileRow(
        entry: entry,
        absolutePath: _terminalPathForGitEntry(workspacePath, entry.path),
        depth: 0,
        showRelativePath: true,
        showAreaMarker: group.unified,
        busy: busy,
        onOpenFile: onOpenFile == null ? null : () => onOpenFile!(entry.path),
        onOpenInZed: onOpenInZed == null
            ? null
            : () => onOpenInZed!(entry.path),
        onRevealInExplorer: () => onRevealInExplorer(entry.path),
        onStage: onStage,
        onUnstage: onUnstage,
        onDiscard: onDiscard,
        submoduleExpanded: submoduleExpanded,
        onToggleSubmodule: () => onToggleSubmodule(entry),
        onTap: entry.isSubmoduleWorktreeOnly
            ? () => onToggleSubmodule(entry)
            : () => unawaited(
                onOpenGitDiff(
                  relativePath: entry.path,
                  area: entry.area,
                  scope: .file,
                  preview: true,
                ),
              ),
      ),
      if (entry.isExpandableSubmodule && submoduleExpanded)
        _SubmoduleChanges(
          workspacePath: workspacePath,
          entry: entry,
          depth: 1,
          busy: busy,
          onOpenGitDiff: onOpenGitDiff,
          onOpenFile: onOpenFile,
          onOpenInZed: onOpenInZed,
          onRevealInExplorer: onRevealInExplorer,
        ),
    ];
    if (children.length == 1) {
      return children.single;
    }
    return Column(
      crossAxisAlignment: .stretch,
      mainAxisSize: .min,
      children: children,
    );
  }
}

String _sectionKeyForGroup(GitChangeGroup group) {
  return group.unified ? 'section:unified' : 'section:${group.area.key}';
}

class const _GitDiffGroupHeader({
  required final GitChangeGroup group,
  required final bool collapsed,
  required final bool busy,
  required final VoidCallback onToggleCollapsed,
  required final VoidCallback onStage,
  required final VoidCallback onUnstage,
  required final VoidCallback onDiscard,
  required final bool canStage,
  required final bool canUnstage,
  required final bool canDiscard,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return _GitDiffBaseRow(
      depth: 0,
      onTap: onToggleCollapsed,
      child: Row(
        children: <Widget>[
          Icon(
            collapsed ? AleraIcons.chevronRight : AleraIcons.chevronDown,
            size: 14,
            color: AleraTokens.foregroundMuted,
          ),
          const SizedBox(width: AleraTokens.space4),
          Expanded(
            child: Text(
              group.label,
              style: Theme.of(context).textTheme.labelSmall
                  ?.copyWith(color: AleraTokens.foregroundMuted),
            ),
          ),
          Text(
            '${group.entries.length}',
            style: Theme.of(context).textTheme.labelSmall
                ?.copyWith(color: AleraTokens.foregroundFaint),
          ),
          const SizedBox(width: AleraTokens.space6),
          _AreaActions(
            busy: busy,
            onStage: onStage,
            onUnstage: onUnstage,
            onDiscard: onDiscard,
            canStage: canStage,
            canUnstage: canUnstage,
            canDiscard: canDiscard,
          ),
        ],
      ),
    );
  }
}

class const _GitDiffMessage({required final String message})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(AleraTokens.space16),
        child: Text(
          message,
          textAlign: .center,
          style: Theme.of(context).textTheme.bodySmall
              ?.copyWith(color: AleraTokens.foregroundMuted),
        ),
      ),
    );
  }
}
