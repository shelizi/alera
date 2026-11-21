part of 'project_workbench_sidebar.dart';

enum _SidebarOrderKind { project, workspace }

class const _SidebarOrderDrag({
  required this.kind,
  required this.id,
  this.projectId,
  this.parentId,
  this.sectionId,
  this.pinned = false,
  this.archived = false,
  this.pinnedCopy = false,
}) {
  final _SidebarOrderKind kind;
  final String id;
  final String? projectId;
  final String? parentId;
  final String? sectionId;
  final bool pinned;
  final bool archived;
  final bool pinnedCopy;
}

/// The row itself remains a normal click target. Only its visible grip can
/// initiate reordering, and Flutter requires the full long-press timeout
/// before a drag avatar appears.
class _SidebarReorderHandle extends StatefulWidget {
  const _SidebarReorderHandle({required this.drag, required this.label});

  final _SidebarOrderDrag drag;
  final String label;

  @override
  State<_SidebarReorderHandle> createState() => _SidebarReorderHandleState();
}

class _SidebarReorderHandleState extends State<_SidebarReorderHandle> {
  static const holdDelay = Duration(milliseconds: 500);
  bool _hovering = false;
  bool _dragging = false;

  @override
  Widget build(BuildContext context) {
    final hint = context.tr('Hold the handle for 0.5s to reorder');
    return Tooltip(
      message: hint,
      waitDuration: const Duration(milliseconds: 350),
      // Tooltip's own long-press recognizer must not compete with the
      // actual LongPressDraggable on touchscreens.
      triggerMode: TooltipTriggerMode.tap,
      child: MouseRegion(
        cursor: SystemMouseCursors.grab,
        onEnter: (_) => setState(() => _hovering = true),
        onExit: (_) => setState(() => _hovering = false),
        child: LongPressDraggable<_SidebarOrderDrag>(
          key: ValueKey(
            'sidebar-reorder-handle:${widget.drag.kind}:${widget.drag.id}',
          ),
          data: widget.drag,
          delay: holdDelay,
          onDragStarted: () => setState(() => _dragging = true),
          onDragEnd: (_) {
            if (mounted) setState(() => _dragging = false);
          },
          feedback: Transform.scale(
            scale: 1.035,
            child: Material(
              color: AleraTokens.surface,
              elevation: 7,
              borderRadius: BorderRadius.circular(AleraTokens.radiusLg),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: AleraTokens.space12,
                  vertical: AleraTokens.space8,
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(Icons.drag_indicator, size: 17),
                    const SizedBox(width: AleraTokens.space8),
                    Text(
                      widget.label,
                      style: Theme.of(context).textTheme.bodyMedium,
                    ),
                  ],
                ),
              ),
            ),
          ),
          childWhenDragging: const SizedBox(width: 24, height: 28),
          child: Semantics(
            label: hint,
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 150),
              width: 24,
              height: 28,
              decoration: BoxDecoration(
                color: _hovering || _dragging
                    ? AleraTokens.surface
                    : Colors.transparent,
                borderRadius: BorderRadius.circular(AleraTokens.radiusMd),
              ),
              child: Icon(
                Icons.drag_indicator,
                size: 17,
                color: _hovering || _dragging
                    ? AleraTokens.foreground
                    : AleraTokens.foregroundFaint,
              ),
            ),
          ),
        ),
      ),
    );
  }
}

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
  required final Future<void> Function(Project project) onSleepProject,
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

  /// Null when the Host has not advertised safe live-PTY transfer. The
  /// sidebar only receives a narrow command, never the Workbench owner.
  final Future<void> Function(WorkbenchTabDragData data, Workspace destination)?
  onTransferTerminalTab,
}) extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (rows.isEmpty) {
      return _EmptyResultsView(query: state.searchQuery);
    }
    // Archived collapse state is session-local, so the rows provider emits
    // every archived row and this layer hides the collapsed ones.
    final expandedArchivedKeys = ref.watch(
      workbenchArchivedSectionsCollapseProvider,
    );
    final archivedCollapsedKeys = defaultCollapsedArchivedSidebarKeys(
      rows,
      expandedArchivedKeys,
    );
    final resolvedEditor = ref.watch(resolvedExternalEditorProvider).value;
    final installedEditors =
        ref.watch(installedExternalEditorsProvider).value ??
        const <ExternalEditorSpec>[];
    final effectiveArchivedCollapsedKeys = archivedSidebarCollapseKeysForQuery(
      archivedCollapsedKeys,
      state.searchQuery,
    );
    final visibleRows = collapseArchivedSidebarRows(
      rows,
      effectiveArchivedCollapsedKeys,
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
            effectiveArchivedCollapsedKeys,
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
          expanded: !archivedSidebarSectionIsCollapsed(
            row.key,
            archivedCollapsedKeys,
          ),
          showTopDivider: row.indent == 0 && previous is WorkbenchWorkspaceRow,
          onToggle: () => ref
              .read(workbenchArchivedSectionsCollapseProvider.notifier)
              .toggle(row.key),
        ),
      );
    }
    if (row is WorkbenchProjectHeaderRow) {
      return _reorderableSidebarRow(
        ref: ref,
        rows: rows,
        label: row.project.name,
        target: _SidebarOrderDrag(kind: .project, id: row.project.id),
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: AleraTokens.space8,
            vertical: AleraTokens.space2,
          ),
          child: _TerminalTabProjectDropTarget(
            project: row.project,
            workspaces:
                state.workspacesByProject[row.project.id] ??
                const <Workspace>[],
            onTransfer: onTransferTerminalTab,
            child: _ProjectHeaderTile(
              project: row.project,
              searchQuery: state.searchQuery,
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
              onSleepProject: () => unawaited(onSleepProject(row.project)),
              onRenameProject: () => onRenameProject(row.project),
              onRemoveProject: () => onRemoveProject(row.project),
            ),
          ),
        ),
      );
    }
    if (row is WorkbenchWorkspaceRow) {
      final leftPadding = _indentPadding(row.indent);
      final hasDescendants = _workspaceHasDescendants(state, row.workspace);
      return _reorderableSidebarRow(
        ref: ref,
        rows: rows,
        label: row.workspace.name,
        target: _SidebarOrderDrag(
          kind: .workspace,
          id: row.workspace.id,
          projectId: row.project.id,
          parentId: row.workspace.parentWorkspaceId,
          sectionId: row.workspace.sectionId,
          pinned: row.workspace.isPinned,
          archived: row.workspace.isArchived,
          pinnedCopy: row.isPinnedCopy,
        ),
        child: Padding(
          padding: EdgeInsets.only(
            left: leftPadding,
            right: AleraTokens.space8,
          ),
          child: _WorkspaceRow(
            project: row.project,
            searchQuery: state.searchQuery,
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
                ? () =>
                      commands.toggleParentWorkspaceCollapsed(row.workspace.id)
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
            onTransferTerminalTab: onTransferTerminalTab,
            onDelete: row.workspace.isMain
                ? null
                : () => onDeleteWorkspace(row.project, row.workspace),
          ),
        ),
      );
    }
    return const SizedBox.shrink();
  }

  Widget _reorderableSidebarRow({
    required WidgetRef ref,
    required List<WorkbenchSidebarRow> rows,
    required String label,
    required _SidebarOrderDrag target,
    required Widget child,
  }) {
    // No cross-project or cross-section moves: drag changes order only, never
    // Workspace parentage, pin/archive state, checkout paths or live PTYs.
    final prefs = state.viewPrefs;
    // A Project grip depends only on Project manual sort, and a Worktree
    // grip depends only on Worktree manual sort. Automatic modes offer no
    // drag target or handle and must never become Manual from a drop.
    final manuallySorted = target.kind == _SidebarOrderKind.project
        ? prefs.projectSort == WorkbenchSortBy.manual
        : prefs.workspaceSort == WorkbenchSortBy.manual;
    if (prefs.groupBy != WorkbenchGroupBy.project ||
        state.searchQuery.trim().isNotEmpty ||
        !manuallySorted) {
      return child;
    }
    bool accepts(_SidebarOrderDrag source) {
      if (source.id == target.id || source.kind != target.kind) return false;
      if (source.kind == _SidebarOrderKind.project) return true;
      return source.projectId == target.projectId &&
          source.parentId == target.parentId &&
          source.sectionId == target.sectionId &&
          source.pinned == target.pinned &&
          source.archived == target.archived &&
          source.pinnedCopy == target.pinnedCopy;
    }

    return DragTarget<_SidebarOrderDrag>(
      onWillAcceptWithDetails: (details) => accepts(details.data),
      onAcceptWithDetails: (details) {
        if (!accepts(details.data)) return;
        final controller = ref.read(workbenchControllerProvider.notifier);
        if (target.kind == _SidebarOrderKind.project) {
          controller.moveProjectInSidebar(
            sourceId: details.data.id,
            targetId: target.id,
            visibleOrder: [
              for (final row in rows)
                if (row is WorkbenchProjectHeaderRow) row.project.id,
            ],
          );
        } else {
          controller.moveWorkspaceInSidebar(
            sourceId: details.data.id,
            targetId: target.id,
            visibleOrder: [
              for (final row in rows)
                if (row is WorkbenchWorkspaceRow &&
                    row.isPinnedCopy == target.pinnedCopy)
                  row.workspace.id,
            ],
          );
        }
      },
      builder: (context, candidates, _) => AnimatedContainer(
        duration: const Duration(milliseconds: 140),
        decoration: BoxDecoration(
          color: candidates.isNotEmpty
              ? AleraTokens.surface
              : Colors.transparent,
          border: Border(
            top: BorderSide(
              color: candidates.isNotEmpty
                  ? AleraTokens.foreground
                  : Colors.transparent,
              width: 2,
            ),
          ),
        ),
        child: Row(
          children: [
            Expanded(child: child),
            _SidebarReorderHandle(drag: target, label: label),
          ],
        ),
      ),
    );
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
