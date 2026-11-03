import 'dart:async';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/feedback/alera_empty_state.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/features/pull_requests/presentation/workspace_pull_requests_panel.dart';
import 'package:alera/src/features/workbench/application/workspace_references_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_git_repositories.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/presentation/workspace_explorer.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_diff_panel.dart';
import 'package:alera/src/features/workbench/presentation/workspace_references_panel.dart';
import 'package:alera/src/features/workbench/presentation/workspace_search_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class const WorkspaceContextSidebar({
  super.key,
  required final Workspace workspace,
  required final WorkbenchViewPrefs prefs,
  final WorkspaceSourceControlScope? sourceControlScope,
  final bool isWorkspaceGitRepository = false,
  final Future<void> Function()? onRefreshRepository,
  final String? focusedSourceControlRoot,
  required final VoidCallback onToggleVisible,
  required final ValueChanged<double> onResize,
  required final ValueChanged<WorkbenchContextPanelTab> onSetContextPanelTab,
  required final ValueChanged<WorkspaceExplorerMode> onSetExplorerMode,
  required final ValueChanged<bool> onSetShowHiddenFiles,
  required final ValueChanged<GitDiffViewMode> onSetGitDiffViewMode,
  required final ValueChanged<GitDiffGroupMode> onSetGitDiffGroupMode,
  final Future<bool> Function(String relativePath)? onFocusSourceControlFolder,
  final VoidCallback? onClearSourceControlRoot,
  required final ValueChanged<String> onOpenFile,
  final ValueChanged<String>? onOpenFilePermanently,
  final ValueChanged<String>? onOpenFileInAlera,
  final ValueChanged<String>? onRevealInExplorer,
  required final OpenGitDiffTabCallback onOpenGitDiff,
  required final OpenGitCommitDiffTabCallback onOpenGitCommitDiff,
  final Future<void> Function(String branch)? onSwitchBranch,
  required final ValueChanged<WorkspaceSearchMatchTarget> onOpenSearchMatch,
  final Future<void> Function(WorkspaceReferenceMatch match)? onOpenReference,
  required final Future<void> Function(
    String oldRelativePath,
    String newRelativePath,
  )
  onPathMoved,
}) extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final sourceControlScope = this.sourceControlScope;
    final activeTab = prefs.activeContextPanelTab;
    final repositoryScan = activeTab == WorkbenchContextPanelTab.gitDiff
        ? ref.watch(workspaceGitRepositoriesProvider(workspace.path))
        : null;
    final repositoryRoots = <String>[
      for (final root in repositoryScan?.asData?.value ?? const <String>[])
        if (root.isNotEmpty || isWorkspaceGitRepository) root,
    ];
    if (isWorkspaceGitRepository && !repositoryRoots.contains('')) {
      repositoryRoots.insert(0, '');
    }
    final selectedRoot = sourceControlScope?.relativeRoot;
    if (selectedRoot != null && !repositoryRoots.contains(selectedRoot)) {
      repositoryRoots.add(selectedRoot);
    }
    return DecoratedBox(
      decoration: const BoxDecoration(
        color: AleraTokens.surfaceVariant,
        border: Border(left: BorderSide(color: AleraTokens.borderSubtle)),
      ),
      child: Stack(
        children: <Widget>[
          Offstage(
            offstage: !prefs.rightSidebarVisible,
            child: _ResizableRightSidebar(
              persistedWidth: prefs.rightSidebarWidth,
              onPersistWidth: onResize,
              child: Column(
                children: <Widget>[
                  _ContextTabHeader(
                    activeTab: activeTab,
                    onSetActiveTab: onSetContextPanelTab,
                    onToggleVisible: onToggleVisible,
                  ),
                  Expanded(
                    child: Stack(
                      fit: StackFit.expand,
                      children: <Widget>[
                        _PersistentWorkspaceExplorerHost(
                          active:
                              prefs.rightSidebarVisible &&
                              activeTab == WorkbenchContextPanelTab.explorer,
                          workspaceIdentity:
                              '${workspace.id}:${workspace.path}',
                          child: WorkspaceExplorer(
                            key: ValueKey<String>(
                              'workspace-explorer:${workspace.id}:${workspace.path}',
                            ),
                            workspace: workspace,
                            mode: prefs.explorerMode,
                            onModeChanged: onSetExplorerMode,
                            showHiddenFiles: prefs.showHiddenFiles,
                            onShowHiddenFilesChanged: onSetShowHiddenFiles,
                            onOpenFile: onOpenFile,
                            onOpenFilePermanently: onOpenFilePermanently,
                            onOpenFileInAlera: onOpenFileInAlera,
                            focusedSourceControlRoot: focusedSourceControlRoot,
                            onFocusSourceControlFolder:
                                onFocusSourceControlFolder,
                            onClearSourceControlRoot: onClearSourceControlRoot,
                            onPathMoved: onPathMoved,
                          ),
                        ),
                        switch (activeTab) {
                          WorkbenchContextPanelTab.explorer =>
                            const SizedBox.shrink(),
                          WorkbenchContextPanelTab.search =>
                            WorkspaceSearchPanel(
                              workspace: workspace,
                              visible: prefs.rightSidebarVisible,
                              onOpenMatch: onOpenSearchMatch,
                            ),
                          WorkbenchContextPanelTab.references =>
                            WorkspaceReferencesPanel(
                              workspace: workspace,
                              onOpenReference: onOpenReference,
                            ),
                          WorkbenchContextPanelTab.gitDiff => Column(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: <Widget>[
                              if (onFocusSourceControlFolder != null)
                                _GitRepositorySelector(
                                  workspacePath: workspace.path,
                                  roots: repositoryRoots,
                                  activeRoot: selectedRoot,
                                  hasActiveRepository:
                                      sourceControlScope != null,
                                  scanning: repositoryScan?.isLoading ?? false,
                                  onSelected: (relativeRoot) {
                                    if (relativeRoot.isEmpty) {
                                      onClearSourceControlRoot?.call();
                                    } else {
                                      unawaited(
                                        onFocusSourceControlFolder!(
                                          relativeRoot,
                                        ),
                                      );
                                    }
                                  },
                                  onRescan: () => ref.invalidate(
                                    workspaceGitRepositoriesProvider(
                                      workspace.path,
                                    ),
                                  ),
                                ),
                              Expanded(
                                child: sourceControlScope == null
                                    ? AleraEmptyState(
                                        icon: AleraIcons.gitBranch,
                                        title: 'Source Control Unavailable',
                                        message: repositoryRoots.isNotEmpty
                                            ? 'Select a Git repository above to show its changes.'
                                            : 'No Git repository selected. Rescan or open a Git repository.',
                                        action: onRefreshRepository == null
                                            ? null
                                            : OutlinedButton.icon(
                                                onPressed: onRefreshRepository,
                                                icon: const Icon(
                                                  AleraIcons.gitRefresh,
                                                ),
                                                label: const Text(
                                                  'Re-detect Git',
                                                ),
                                              ),
                                      )
                                    : WorkspaceGitDiffPanel(
                                        workspace: workspace,
                                        sourceControlScope: sourceControlScope,
                                        onRefreshRepository:
                                            onRefreshRepository,
                                        viewMode: prefs.gitDiffViewMode,
                                        onViewModeChanged: onSetGitDiffViewMode,
                                        groupMode: prefs.gitDiffGroupMode,
                                        onGroupModeChanged:
                                            onSetGitDiffGroupMode,
                                        onOpenGitDiff: onOpenGitDiff,
                                        onOpenGitCommitDiff:
                                            onOpenGitCommitDiff,
                                        onSwitchBranch: onSwitchBranch,
                                        onOpenFile:
                                            onOpenFilePermanently ?? onOpenFile,
                                        onRevealInExplorer: onRevealInExplorer,
                                        onClearSourceControlRoot:
                                            sourceControlScope.isWorkspaceRoot
                                            ? null
                                            : onClearSourceControlRoot,
                                      ),
                              ),
                            ],
                          ),
                          WorkbenchContextPanelTab.pullRequests =>
                            sourceControlScope == null
                                ? const AleraEmptyState(
                                    icon: AleraIcons.gitPullRequest,
                                    title: 'Pull Request Unavailable',
                                    message: 'This workspace is not connected to a Git repository, so there are no Pull Requests to show.',
                                  )
                                : WorkspacePullRequestsPanel(
                                    key: ValueKey<String>(
                                      'workspace-pull-requests:${workspace.id}:${sourceControlScope.path}',
                                    ),
                                    workspace: workspace,
                                    repoPath: sourceControlScope.path,
                                    gitDiffRoot:
                                        sourceControlScope.relativeRoot,
                                  ),
                        },
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
          if (!prefs.rightSidebarVisible)
            _CollapsedContextRail(
              activeTab: activeTab,
              onOpenTab: (tab) {
                onSetContextPanelTab(tab);
                onToggleVisible();
              },
              onToggleVisible: onToggleVisible,
            ),
        ],
      ),
    );
  }
}

class const _PersistentWorkspaceExplorerHost({
  required final bool active,
  required final String workspaceIdentity,
  required final Widget child,
}) extends StatefulWidget {
  @override
  State<_PersistentWorkspaceExplorerHost> createState() =>
      _PersistentWorkspaceExplorerHostState();
}

class _PersistentWorkspaceExplorerHostState
    extends State<_PersistentWorkspaceExplorerHost> {
  late bool _mounted = widget.active;

  @override
  void didUpdateWidget(covariant _PersistentWorkspaceExplorerHost oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.workspaceIdentity != widget.workspaceIdentity) {
      _mounted = widget.active;
    } else if (widget.active) {
      _mounted = true;
    }
  }

  @override
  Widget build(BuildContext context) {
    if (!_mounted) {
      return const SizedBox.shrink();
    }
    return Offstage(offstage: !widget.active, child: widget.child);
  }
}

class const _ResizableRightSidebar({
  required final double persistedWidth,
  required final ValueChanged<double> onPersistWidth,
  required final Widget child,
}) extends StatefulWidget {
  @override
  State<_ResizableRightSidebar> createState() => _ResizableRightSidebarState();
}

class _ResizableRightSidebarState extends State<_ResizableRightSidebar> {
  double? _transientWidth;

  double get _width => _transientWidth ?? widget.persistedWidth;

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: .stretch,
      children: <Widget>[
        _RightResizeHandle(
          currentWidth: _width,
          onResize: (width) {
            setState(() {
              _transientWidth = width.clamp(
                AleraTokens.sidebarMinWidth,
                AleraTokens.sidebarMaxWidth,
              );
            });
          },
          onResizeEnd: (width) {
            widget.onPersistWidth(width);
            setState(() => _transientWidth = null);
          },
        ),
        SizedBox(width: _width, child: widget.child),
      ],
    );
  }
}

class const _CollapsedContextRail({
  required final WorkbenchContextPanelTab activeTab,
  required final ValueChanged<WorkbenchContextPanelTab> onOpenTab,
  required final VoidCallback onToggleVisible,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: AleraTokens.sidebarCollapsedWidth,
      child: Column(
        children: <Widget>[
          const SizedBox(height: AleraTokens.space8),
          _ContextTabButton(
            tab: .explorer,
            activeTab: activeTab,
            tooltip: 'Explorer',
            icon: AleraIcons.copyFiles,
            onPressed: () => onOpenTab(.explorer),
          ),
          const SizedBox(height: AleraTokens.space6),
          _ContextTabButton(
            tab: .search,
            activeTab: activeTab,
            tooltip: 'Search',
            icon: AleraIcons.search,
            onPressed: () => onOpenTab(.search),
          ),
          const SizedBox(height: AleraTokens.space6),
          _ContextTabButton(
            tab: .references,
            activeTab: activeTab,
            tooltip: 'References',
            icon: AleraIcons.listView,
            onPressed: () => onOpenTab(.references),
          ),
          const SizedBox(height: AleraTokens.space6),
          _ContextTabButton(
            tab: .gitDiff,
            activeTab: activeTab,
            tooltip: 'Source Control',
            icon: AleraIcons.gitBranch,
            onPressed: () => onOpenTab(.gitDiff),
          ),
          const SizedBox(height: AleraTokens.space6),
          _ContextTabButton(
            tab: .pullRequests,
            activeTab: activeTab,
            tooltip: 'Pull Request',
            icon: AleraIcons.gitPullRequest,
            onPressed: () => onOpenTab(.pullRequests),
          ),
          const Spacer(),
          Padding(
            padding: const EdgeInsets.only(bottom: AleraTokens.space8),
            child: AleraIconButton(
              tooltip: 'Expand panel',
              icon: AleraIcons.chevronsLeft,
              onPressed: onToggleVisible,
            ),
          ),
        ],
      ),
    );
  }
}

class const _ContextTabHeader({
  required final WorkbenchContextPanelTab activeTab,
  required final ValueChanged<WorkbenchContextPanelTab> onSetActiveTab,
  required final VoidCallback onToggleVisible,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: const BoxDecoration(
        color: AleraTokens.surfaceVariant,
        border: Border(bottom: BorderSide(color: AleraTokens.borderSubtle)),
      ),
      child: SizedBox(
        height: AleraTokens.sidebarHeaderHeight,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: AleraTokens.space8),
          child: Row(
            children: <Widget>[
              _ContextTabButton(
                tab: .explorer,
                activeTab: activeTab,
                tooltip: 'Explorer',
                icon: AleraIcons.copyFiles,
                onPressed: () => onSetActiveTab(.explorer),
              ),
              const SizedBox(width: AleraTokens.space6),
              _ContextTabButton(
                tab: .search,
                activeTab: activeTab,
                tooltip: 'Search',
                icon: AleraIcons.search,
                onPressed: () => onSetActiveTab(.search),
              ),
              const SizedBox(width: AleraTokens.space6),
              _ContextTabButton(
                tab: .references,
                activeTab: activeTab,
                tooltip: 'References',
                icon: AleraIcons.listView,
                onPressed: () => onSetActiveTab(.references),
              ),
              const SizedBox(width: AleraTokens.space6),
              _ContextTabButton(
                tab: .gitDiff,
                activeTab: activeTab,
                tooltip: 'Source Control',
                icon: AleraIcons.gitBranch,
                onPressed: () => onSetActiveTab(.gitDiff),
              ),
              const SizedBox(width: AleraTokens.space6),
              _ContextTabButton(
                tab: .pullRequests,
                activeTab: activeTab,
                tooltip: 'Pull Request',
                icon: AleraIcons.gitPullRequest,
                onPressed: () => onSetActiveTab(.pullRequests),
              ),
              const Spacer(),
              AleraIconButton(
                tooltip: 'Collapse panel',
                icon: AleraIcons.chevronsRight,
                onPressed: onToggleVisible,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class const _ContextTabButton({
  required final WorkbenchContextPanelTab tab,
  required final WorkbenchContextPanelTab activeTab,
  required final String tooltip,
  required final IconData icon,
  required final VoidCallback onPressed,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final active = tab == activeTab;
    return AleraIconButton(
      tooltip: tooltip,
      icon: icon,
      onPressed: onPressed,
      iconColor: active ? AleraTokens.foreground : AleraTokens.foregroundMuted,
      backgroundColor: active ? AleraTokens.surfaceElevated : null,
      borderColor: active ? AleraTokens.border : AleraTokens.borderSubtle,
    );
  }
}

class const _RightResizeHandle({
  required final double currentWidth,
  required final ValueChanged<double> onResize,
  required final ValueChanged<double> onResizeEnd,
}) extends StatefulWidget {
  @override
  State<_RightResizeHandle> createState() => _RightResizeHandleState();
}

class _RightResizeHandleState extends State<_RightResizeHandle> {
  bool _hovered = false;
  bool _dragging = false;
  double? _dragWidth;

  @override
  Widget build(BuildContext context) {
    final active = _hovered || _dragging;
    return MouseRegion(
      cursor: SystemMouseCursors.resizeColumn,
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: GestureDetector(
        behavior: .translucent,
        onHorizontalDragStart: (_) {
          _dragWidth = widget.currentWidth;
          setState(() => _dragging = true);
        },
        onHorizontalDragEnd: (_) => _stopDragging(),
        onHorizontalDragCancel: _stopDragging,
        onHorizontalDragUpdate: (details) {
          final next = (_dragWidth ?? widget.currentWidth) - details.delta.dx;
          _dragWidth = next;
          widget.onResize(next);
        },
        child: SizedBox(
          width: AleraTokens.space6,
          child: Center(
            child: AnimatedContainer(
              duration: AleraTokens.durationFast,
              width: active ? 2 : 1,
              color: active ? AleraTokens.border : AleraTokens.borderSubtle,
            ),
          ),
        ),
      ),
    );
  }

  void _stopDragging() {
    final finalWidth = _dragWidth ?? widget.currentWidth;
    _dragWidth = null;
    setState(() => _dragging = false);
    widget.onResizeEnd(finalWidth);
  }
}

/// Shows the workspace's repositories without merging their Git state.
class const _GitRepositorySelector({
  required this.workspacePath,
  required this.roots,
  required this.activeRoot,
  required this.hasActiveRepository,
  required this.scanning,
  required this.onSelected,
  required this.onRescan,
}) extends ConsumerStatefulWidget {
  final String workspacePath;
  final List<String> roots;
  final String? activeRoot;
  final bool hasActiveRepository;
  final bool scanning;
  final ValueChanged<String> onSelected;
  final VoidCallback onRescan;

  @override
  ConsumerState<_GitRepositorySelector> createState() =>
      _GitRepositorySelectorState();
}

class _GitRepositorySelectorState
    extends ConsumerState<_GitRepositorySelector> {
  bool _overviewExpanded = false;

  @override
  Widget build(BuildContext context) {
    final summary = _overviewExpanded
        ? ref.watch(
            workspaceGitRepositoryOverviewProvider(widget.workspacePath),
          )
        : null;
    final overviews =
        summary?.asData?.value ??
        const <String, GitRepositoryOverviewSummary>{};
    final label = widget.hasActiveRepository
        ? _labelForRoot(context, widget.activeRoot ?? '')
        : widget.scanning
        ? context.tr('Finding Git Repositories...')
        : context.tr('Select Git Repository');

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        DecoratedBox(
          decoration: const BoxDecoration(
            border: Border(bottom: BorderSide(color: AleraTokens.borderSubtle)),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: AleraTokens.space8,
              vertical: AleraTokens.space4,
            ),
            child: Row(
              children: <Widget>[
                Expanded(
                  child: PopupMenuButton<String>(
                    key: const ValueKey<String>(
                      'workspace-git-repository-selector',
                    ),
                    enabled: widget.roots.isNotEmpty,
                    tooltip: context.tr('Switch Git Repository'),
                    onSelected: _selectRoot,
                    itemBuilder: (context) => <PopupMenuEntry<String>>[
                      for (final root in widget.roots)
                        PopupMenuItem<String>(
                          value: root,
                          child: Row(
                            children: <Widget>[
                              Icon(
                                _isSelected(root)
                                    ? Icons.check
                                    : AleraIcons.gitBranch,
                                size: 16,
                              ),
                              const SizedBox(width: AleraTokens.space8),
                              Expanded(
                                child: Text(
                                  _labelForRoot(context, root),
                                  overflow: TextOverflow.ellipsis,
                                  maxLines: 1,
                                ),
                              ),
                            ],
                          ),
                        ),
                    ],
                    child: Row(
                      children: <Widget>[
                        const Icon(AleraIcons.gitBranch, size: 16),
                        const SizedBox(width: AleraTokens.space8),
                        Expanded(
                          child: Text(
                            label,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                        const Icon(Icons.arrow_drop_down, size: 18),
                      ],
                    ),
                  ),
                ),
                Text(
                  widget.roots.length.toString(),
                  style: Theme.of(context).textTheme.labelSmall
                      ?.copyWith(color: AleraTokens.foregroundFaint),
                ),
                IconButton(
                  key: const ValueKey<String>(
                    'git-repositories-overview-toggle',
                  ),
                  tooltip: _overviewExpanded
                      ? context.tr('Hide Git Repositories')
                      : context.tr('Show Git Repositories'),
                  icon: Icon(
                    _overviewExpanded ? Icons.expand_less : Icons.expand_more,
                    size: 18,
                  ),
                  visualDensity: VisualDensity.compact,
                  onPressed: widget.roots.isEmpty
                      ? null
                      : () => setState(
                          () => _overviewExpanded = !_overviewExpanded,
                        ),
                ),
                IconButton(
                  tooltip: context.tr('Rescan Git Repositories'),
                  icon: const Icon(AleraIcons.gitRefresh, size: 18),
                  visualDensity: VisualDensity.compact,
                  onPressed: widget.scanning ? null : widget.onRescan,
                ),
              ],
            ),
          ),
        ),
        if (_overviewExpanded)
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 210),
            child: ListView.builder(
              shrinkWrap: true,
              itemCount: widget.roots.length,
              itemBuilder: (context, index) {
                final root = widget.roots[index];
                final detail = overviews[root];
                final statusLabel = summary?.hasError ?? false
                    ? context.tr('Status Unavailable')
                    : detail == null
                    ? context.tr('Loading...')
                    : !detail.available
                    ? context.tr('Status Unavailable')
                    : _summaryLine(detail, context);
                return InkWell(
                  key: ValueKey<String>('git-repo-overview:$root'),
                  onTap: () => _selectRoot(root),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: AleraTokens.space8,
                      vertical: AleraTokens.space6,
                    ),
                    child: Row(
                      children: <Widget>[
                        Icon(
                          _isSelected(root)
                              ? Icons.radio_button_checked
                              : Icons.radio_button_unchecked,
                          size: 16,
                          color: _isSelected(root)
                              ? AleraTokens.foreground
                              : AleraTokens.foregroundMuted,
                        ),
                        const SizedBox(width: AleraTokens.space8),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: <Widget>[
                              Text(
                                _labelForRoot(context, root),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              ),
                              Text(
                                statusLabel,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: Theme.of(context).textTheme.labelSmall
                                    ?.copyWith(
                                      color: AleraTokens.foregroundFaint,
                                    ),
                              ),
                            ],
                          ),
                        ),
                        if (detail?.available == true &&
                            detail!.changedFiles > 0)
                          Text(
                            detail.changedFiles.toString(),
                            style: Theme.of(context).textTheme.labelMedium,
                          ),
                      ],
                    ),
                  ),
                );
              },
            ),
          ),
      ],
    );
  }

  bool _isSelected(String root) =>
      widget.hasActiveRepository && root == (widget.activeRoot ?? '');

  void _selectRoot(String root) {
    if (_isSelected(root)) return;
    widget.onSelected(root);
  }

  static String _labelForRoot(BuildContext context, String root) =>
      root.isEmpty ? context.tr('Workspace Root') : root;

  static String _summaryLine(
    GitRepositoryOverviewSummary value,
    BuildContext context,
  ) {
    final parts = <String>[
      value.branch!,
      if (value.changedFiles == 0)
        context.tr('Clean')
      else
        value.changedFiles.toString() + ' ' + context.tr('files'),
      if (value.ahead > 0) '↑' + value.ahead.toString(),
      if (value.behind > 0) '↓' + value.behind.toString(),
      if (value.conflicts) context.tr('Conflicts'),
    ];
    return parts.join(' · ');
  }
}
