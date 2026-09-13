part of 'project_workbench_sidebar.dart';

class const _SidebarBody({
  required final WorkbenchState state,
  required final _WorkbenchSidebarCommands commands,
  required final List<WorkbenchSidebarRow> rows,
  required final Future<void> Function(Project project, Workspace workspace)
  onOpenWorkspace,
  required final Future<void> Function(Workspace workspace)
  onOpenWorkspaceFolder,
  required final Future<void> Function(
    Workspace workspace,
    ExternalEditorKind kind,
  )
  onOpenWorkspaceExternally,
  required final Future<void> Function(Workspace workspace) onCopyWorkspacePath,
  required final Future<void> Function(Workspace workspace)
  onOpenWorkspaceInBrowser,
  required final Future<void> Function(Workspace workspace) onSleepWorkspace,
  required final Future<void> Function(Workspace workspace) onArchiveWorkspace,
  required final Future<void> Function(Workspace workspace) onRestoreWorkspace,
  required final Future<void> Function(Project project) onCreateWorkspace,
  required final Future<void> Function(Project project) onOpenProjectSettings,
  required final Future<void> Function(Project project, Workspace workspace)
  onDeleteWorkspace,
  required final Future<void> Function(Project project) onRenameProject,
  required final Future<void> Function(Project project) onRemoveProject,
  required final Future<void> Function(Workspace workspace) onRenameWorkspace,
  required final Future<void> Function(Workspace workspace, bool isPinned)
  onSetWorkspacePinned,
  required final Future<void> Function(Workspace workspace, bool isPinned)
  onSetWorkspaceTreePinned,
  required final Future<void> Function(Workspace workspace)
  onManageWorkspaceTags,
  required final Future<void> Function(Workspace workspace)
  onSetWorkspaceParent,
  required final Future<void> Function(Workspace workspace)
  onClearWorkspaceParent,
  required final String fileManagerLabel,
  required final _TerminalTabCallback onSelectTerminal,
  required final _TerminalTabCallback onCloseTerminal,
}) extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (rows.isEmpty) {
      return _EmptyResultsView(query: state.searchQuery);
    }
    // Archived collapse state is session-local, so the rows provider emits
    // every archived row and this layer hides the collapsed ones.
    final archivedCollapsedKeys = ref.watch(
      workbenchArchivedSectionsCollapseProvider,
    );
    final resolvedEditor = ref.watch(resolvedExternalEditorProvider).value;
    final installedEditors =
        ref.watch(installedExternalEditorsProvider).value ??
        const <ExternalEditorSpec>[];
    final visibleRows = collapseArchivedSidebarRows(
      rows,
      archivedCollapsedKeys,
    );
    return ListView.builder(
      padding: const EdgeInsets.only(
        top: AleraTokens.space4,
        bottom: AleraTokens.space8,
      ),
      itemCount: visibleRows.length,
      itemBuilder: (context, index) {
        return KeyedSubtree(
          key: ValueKey<String>(visibleRows[index].key),
          child: _buildRow(
            context,
            ref,
            visibleRows,
            index,
            archivedCollapsedKeys,
            resolvedEditor,
            installedEditors,
          ),
        );
      },
    );
  }

  Widget _buildRow(
    BuildContext context,
    WidgetRef ref,
    List<WorkbenchSidebarRow> rows,
    int index,
    Set<String> archivedCollapsedKeys,
    ExternalEditorSpec? resolvedEditor,
    List<ExternalEditorSpec> installedEditors,
  ) {
    final row = rows[index];
    if (row is WorkbenchSectionHeaderRow) {
      return _WorkspaceSectionHeader(row: row, commands: commands);
    }
    if (row is WorkbenchPinnedHeaderRow) {
      return _SidebarSectionTile(
        leadingIcon: AleraIcons.pin,
        label: 'Pinned',
        count: row.workspaceCount,
        expanded: !row.collapsed,
        onToggle: commands.togglePinnedSectionCollapsed,
      );
    }
    if (row is WorkbenchAllHeaderRow) {
      final previous = index > 0 ? rows[index - 1] : null;
      return _SidebarSectionTile(
        leadingIcon: AleraIcons.listView,
        label: 'All',
        count: row.workspaceCount,
        expanded: !row.collapsed,
        showTopDivider: previous is WorkbenchWorkspaceRow,
        onToggle: commands.toggleAllSectionCollapsed,
      );
    }
    if (row is WorkbenchArchivedHeaderRow) {
      final previous = index > 0 ? rows[index - 1] : null;
      return Padding(
        padding: EdgeInsets.only(left: AleraTokens.space12 * row.indent),
        child: _SidebarSectionTile(
          leadingIcon: AleraIcons.archive,
          label: 'Archived',
          count: row.workspaceCount,
          expanded: !archivedCollapsedKeys.contains(row.key),
          showTopDivider: row.indent == 0 && previous is WorkbenchWorkspaceRow,
          onToggle: () => ref
              .read(workbenchArchivedSectionsCollapseProvider.notifier)
              .toggle(row.key),
        ),
      );
    }
    if (row is WorkbenchProjectHeaderRow) {
      return Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: AleraTokens.space8,
          vertical: AleraTokens.space2,
        ),
        child: _ProjectHeaderTile(
          project: row.project,
          expanded: !row.collapsed,
          workspaceCount: row.workspaceCount,
          agentCounts: row.agentCounts,
          onToggle: () => commands.toggleProjectCollapsed(row.project.id),
          onCreateWorkspace: row.project.supportsLinkedWorkspaces
              ? () => onCreateWorkspace(row.project)
              : null,
          onRefreshWorktrees: row.project.supportsLinkedWorkspaces
              ? () => unawaited(
                  commands.reconcileProjectWorkspaces(row.project.id),
                )
              : null,
          onOpenProjectSettings: () =>
              unawaited(onOpenProjectSettings(row.project)),
          onRenameProject: () => onRenameProject(row.project),
          onRemoveProject: () => onRemoveProject(row.project),
        ),
      );
    }
    if (row is WorkbenchWorkspaceRow) {
      final leftPadding = _indentPadding(row.indent);
      final hasDescendants = _workspaceHasDescendants(state, row.workspace);
      return Padding(
        padding: EdgeInsets.only(left: leftPadding, right: AleraTokens.space8),
        child: _WorkspaceRow(
          project: row.project,
          workspace: row.workspace,
          agentRuns: row.agentRuns,
          agentRunGroups: row.agentRunGroups,
          status: row.aggregateStatus,
          hasTerminalTabs: row.hasTerminalTabs,
          isActive: row.workspace.id == state.activeWorkspaceId,
          activeTabId: state.activeTabIdByWorkspace[row.workspace.id],
          showProject: row.showProjectChip,
          expanded: row.expanded,
          visibleChildCount: row.visibleChildCount,
          childrenCollapsed: row.childrenCollapsed,
          isPinnedCopy: row.isPinnedCopy,
          onToggleChildren: row.hasVisibleChildren
              ? () => commands.toggleParentWorkspaceCollapsed(row.workspace.id)
              : null,
          onTap: () => row.workspace.isArchived
              ? unawaited(onRestoreWorkspace(row.workspace))
              : unawaited(onOpenWorkspace(row.project, row.workspace)),
          onOpenFolder: () => unawaited(onOpenWorkspaceFolder(row.workspace)),
          onOpenExternally: (kind) =>
              unawaited(onOpenWorkspaceExternally(row.workspace, kind)),
          externalEditor: resolvedEditor,
          installedExternalEditors: installedEditors,
          onCopyPath: () => unawaited(onCopyWorkspacePath(row.workspace)),
          onOpenInBrowser: () =>
              unawaited(onOpenWorkspaceInBrowser(row.workspace)),
          onOpenProjectSettings: () =>
              unawaited(onOpenProjectSettings(row.project)),
          onSleep: () => onSleepWorkspace(row.workspace),
          onArchive: () => onArchiveWorkspace(row.workspace),
          onRestore: () => onRestoreWorkspace(row.workspace),
          onToggleExpanded: () =>
              commands.toggleWorkspaceExpanded(row.workspace.id),
          fileManagerLabel: fileManagerLabel,
          onRename: () => onRenameWorkspace(row.workspace),
          onSetPinned: () =>
              onSetWorkspacePinned(row.workspace, !row.workspace.isPinned),
          onPinWorkspaceTree: hasDescendants
              ? () => onSetWorkspaceTreePinned(row.workspace, true)
              : null,
          onUnpinWorkspaceTree: hasDescendants
              ? () => onSetWorkspaceTreePinned(row.workspace, false)
              : null,
          onManageTags: () => onManageWorkspaceTags(row.workspace),
          onSetSection: state.supportsSections
              ? () => commands.showWorkspaceSection(row.workspace)
              : null,
          onClearSection:
              state.supportsSections && row.workspace.sectionId != null
              ? () => commands.clearWorkspaceSection(row.workspace)
              : null,
          onSetParent: () => onSetWorkspaceParent(row.workspace),
          onClearParent: row.workspace.hasParentWorkspace
              ? () => onClearWorkspaceParent(row.workspace)
              : null,
          onSelectTerminal: onSelectTerminal,
          onCloseTerminal: onCloseTerminal,
          onDelete: row.workspace.isMain
              ? null
              : () => onDeleteWorkspace(row.project, row.workspace),
        ),
      );
    }
    return const SizedBox.shrink();
  }

  bool _workspaceHasDescendants(WorkbenchState state, Workspace workspace) {
    return workspaceIdsDescendedFrom([
      for (final group in state.workspacesByProject.values) ...group,
    ], workspace.id).isNotEmpty;
  }

  /// Base sidebar padding plus one row-content step per nesting level, clamped
  /// so deep trees keep usable row widths in a narrow sidebar.
  double _indentPadding(int indent) {
    const double base = AleraTokens.space8;
    const double step = AleraTokens.space12;
    const double max = base + 4 * step;
    final padding = base + indent * step;
    return padding > max ? max : padding;
  }
}

class const _EmptyResultsView({required final String query})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final trimmed = query.trim();
    final message = trimmed.isEmpty
        ? 'No workspaces match the current filters'
        : 'No workspaces match "$trimmed"';
    return AleraEmptyState(message: message);
  }
}
