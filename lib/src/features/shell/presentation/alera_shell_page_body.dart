part of 'alera_shell_page.dart';

class _AleraShellPageBodyState extends ConsumerState<_AleraShellPageBody> {
  String? _lastErrorMessage;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      await ref.read(workbenchControllerProvider.notifier).bootstrap();
    });
  }

  @override
  Widget build(BuildContext context) {
    ref.watch(terminalHostWarmupCoordinatorProvider);
    ref.watch(runtimeAgentStatusSyncProvider);
    ref.watch(agentHookInstallerCoordinatorProvider);
    ref.watch(agentStatusNotificationCoordinatorProvider);
    ref.watch(workspacePullRequestMonitorControllerProvider.notifier);
    ref.watch(workspacePullRequestFailureNotificationCoordinatorProvider);
    ref.watch(agentAwakeCoordinatorProvider);
    ref.watch(keepAliveCoordinatorProvider);
    ref.watch(terminalRuntimeExitCoordinatorProvider);
    ref.watch(workspaceActivityCoordinatorProvider);
    ref.watch(terminalRuntimeActiveWorkspaceCoordinatorProvider);
    ref.watch(workspaceActivityPersistenceCoordinatorProvider);
    ref.watch(workbenchArchiveSweepCoordinatorProvider);
    ref.watch(workspaceLanguageIntelligencePrewarmCoordinatorProvider);
    final shell = ref.watch(
      workbenchControllerProvider.select((state) {
        final workspace = state.activeWorkspace;
        return (
          activeProject: state.activeProject,
          activeWorkspace: workspace,
          bootstrapped: state.bootstrapped,
          collapsed: state.collapsed,
          error: state.error,
          hasProjects: state.projects.isNotEmpty,
          layout: workspace == null ? null : state.layoutFor(workspace.id),
          tabs: workspace == null
              ? const <WorkspaceTabRecord>[]
              : state.tabsFor(workspace.id),
          tabsByWorkspace: state.tabsByWorkspace,
          viewPrefs: state.viewPrefs,
        );
      }),
    );
    // Prune acknowledgements whose terminal session was closed. Listening
    // instead of mutating inside build keeps provider writes out of the
    // widget life-cycle; the provider starts empty, so there is nothing to
    // prune until tabs actually change.
    ref.listen<Map<String, List<WorkspaceTabRecord>>>(
      workbenchControllerProvider.select((state) => state.tabsByWorkspace),
      (_, tabsByWorkspace) {
        ref
            .read(
              workbenchTabCompletionAcknowledgementsControllerProvider.notifier,
            )
            .retainTerminalSessions(<String>{
              for (final tabs in tabsByWorkspace.values)
                for (final tab in tabs)
                  if (tab.kind == WorkspaceTabKind.terminal)
                    tab.terminalSessionId,
            });
      },
    );
    final error = shell.error;
    if (error != null && error != _lastErrorMessage) {
      _lastErrorMessage = error;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) {
          return;
        }
        _showError(error);
      });
    }

    final project = shell.activeProject;
    final workspace = shell.activeWorkspace;
    final controller = ref.read(workbenchControllerProvider.notifier);
    final defaultCodeOpenTarget = ref
        .watch(settingsControllerProvider)
        .editor
        .codeOpenTarget;
    final canSelectSourceControlRoot = project?.isFolder == true;
    final sourceControlScope = WorkspaceSourceControlScope.resolve(
      project: project,
      workspace: workspace,
      prefs: shell.viewPrefs,
    );

    final content = AleraAppMenuScope(
      child: Scaffold(
        body: KeyboardShortcutsScope(
          child: LayoutBuilder(
            builder: (context, constraints) {
              final showContextSidebar =
                  workspace != null &&
                  _canShowContextSidebar(
                    shellWidth: constraints.maxWidth,
                    collapsed: shell.collapsed,
                    prefs: shell.viewPrefs,
                  );
              return Column(
                children: <Widget>[
                  Expanded(
                    child: Row(
                      crossAxisAlignment: .stretch,
                      children: <Widget>[
                        const ProjectWorkbenchSidebar(),
                        Expanded(
                          child: _buildContent(
                            bootstrapped: shell.bootstrapped,
                            hasProjects: shell.hasProjects,
                            project: project,
                            workspace: workspace,
                            sourceControlScope: sourceControlScope,
                            tabs: shell.tabs,
                            layout: shell.layout,
                          ),
                        ),
                        if (workspace != null && showContextSidebar)
                          WorkspaceContextSidebar(
                            workspace: workspace,
                            prefs: shell.viewPrefs,
                            sourceControlScope: sourceControlScope,
                            focusedSourceControlRoot: canSelectSourceControlRoot
                                ? shell
                                      .viewPrefs
                                      .sourceControlRootByWorkspaceId[workspace
                                      .id]
                                : null,
                            onToggleVisible:
                                controller.toggleRightSidebarVisible,
                            onResize: controller.setRightSidebarWidth,
                            onSetContextPanelTab: controller.setContextPanelTab,
                            onSetExplorerMode: controller.setExplorerMode,
                            onSetShowHiddenFiles: controller.setShowHiddenFiles,
                            onSetGitDiffViewMode: controller.setGitDiffViewMode,
                            onSetGitDiffGroupMode:
                                controller.setGitDiffGroupMode,
                            onSwitchBranch: project == null
                                ? null
                                : (branch) async {
                                    await controller.switchWorkspaceBranch(
                                      project: project,
                                      workspace: workspace,
                                      branch: branch,
                                    );
                                  },
                            onFocusSourceControlFolder:
                                canSelectSourceControlRoot
                                ? (relativePath) {
                                    return controller.focusSourceControlFolder(
                                      workspace: workspace,
                                      relativePath: relativePath,
                                    );
                                  }
                                : null,
                            onClearSourceControlRoot: canSelectSourceControlRoot
                                ? () {
                                    controller.clearFocusedSourceControlFolder(
                                      workspace: workspace,
                                    );
                                  }
                                : null,
                            onOpenFile: (relativePath) {
                              unawaited(
                                _openWorkspaceFile(
                                  workspace: workspace,
                                  relativePath: relativePath,
                                ),
                              );
                            },
                            onOpenFilePermanently: (relativePath) {
                              unawaited(
                                _openWorkspaceFile(
                                  workspace: workspace,
                                  relativePath: relativePath,
                                ),
                              );
                            },
                            onOpenFileInAlera:
                                defaultCodeOpenTarget == CodeOpenTarget.external
                                ? (relativePath) {
                                    unawaited(
                                      _openWorkspaceFile(
                                        workspace: workspace,
                                        relativePath: relativePath,
                                        targetOverride: CodeOpenTarget.alera,
                                      ),
                                    );
                                  }
                                : null,
                            onOpenGitDiff:
                                ({
                                  relativePath,
                                  area,
                                  gitDiffRoot,
                                  required scope,
                                  preview = false,
                                }) {
                                  return controller.openGitDiffTab(
                                    workspace: workspace,
                                    relativePath: relativePath,
                                    area: area,
                                    scope: scope,
                                    gitDiffRoot: gitDiffRoot,
                                    preview: preview,
                                  );
                                },
                            onOpenGitCommitDiff:
                                ({
                                  relativePath,
                                  oldPath,
                                  required scope,
                                  gitDiffRoot,
                                  required commitOid,
                                  parentOid,
                                  required compareRef,
                                  subject,
                                  message,
                                  preview = false,
                                }) {
                                  return controller.openGitCommitDiffTab(
                                    workspace: workspace,
                                    relativePath: relativePath,
                                    oldPath: oldPath,
                                    scope: scope,
                                    gitDiffRoot: gitDiffRoot,
                                    commitOid: commitOid,
                                    parentOid: parentOid,
                                    compareRef: compareRef,
                                    subject: subject,
                                    message: message,
                                    preview: preview,
                                  );
                                },
                            onOpenSearchMatch: (target) {
                              unawaited(() async {
                                final result = await ref
                                    .read(workspaceFileOpenCoordinatorProvider)
                                    .open<WorkspaceTabRecord>(
                                      workspace: workspace,
                                      relativePath: target.relativePath,
                                      line: target.line,
                                      column: target.column,
                                      openInAlera:
                                          ({
                                            required workspace,
                                            required relativePath,
                                            required preview,
                                          }) => controller.openEditorTab(
                                            workspace: workspace,
                                            relativePath: relativePath,
                                            preview: preview,
                                          ),
                                    );
                                final tab = result.internalValue;
                                if (tab == null) return;
                                ref
                                    .read(editorSessionRegistryProvider)
                                    .reveal(
                                      tab.id,
                                      WorkspaceEditorRevealTarget(
                                        line: target.line,
                                        column: target.column,
                                        matchLength: target.matchLength,
                                      ),
                                    );
                              }());
                            },
                            onOpenReference: (match) async {
                              final tab = await controller.openEditorTab(
                                workspace: workspace,
                                relativePath: match.relativePath,
                                preview: false,
                              );
                              ref
                                  .read(editorSessionRegistryProvider)
                                  .reveal(
                                    tab.id,
                                    WorkspaceEditorRevealTarget(
                                      line: match.line,
                                      column: match.column,
                                      matchLength: match.matchLength,
                                    ),
                                  );
                            },
                            onPathMoved:
                                (oldRelativePath, newRelativePath) async {
                                  await controller.syncFileTabsAfterPathMove(
                                    workspace: workspace,
                                    oldRelativePath: oldRelativePath,
                                    newRelativePath: newRelativePath,
                                  );
                                  ref
                                      .read(editorSessionRegistryProvider)
                                      .updateDocumentPathsAfterMove(
                                        workspacePath: workspace.path,
                                        oldRelativePath: oldRelativePath,
                                        newRelativePath: newRelativePath,
                                      );
                                  controller.syncSourceControlRootAfterPathMove(
                                    workspace: workspace,
                                    oldRelativePath: oldRelativePath,
                                    newRelativePath: newRelativePath,
                                  );
                                },
                          ),
                      ],
                    ),
                  ),
                  AgentQuotaStatusBar(
                    trailing: Row(
                      mainAxisSize: .min,
                      children: <Widget>[
                        if (project?.isGitRepository == true &&
                            workspace != null &&
                            sourceControlScope != null)
                          WorkspaceBranchStatusBarControl(
                            workspace: workspace,
                            sourceControlScope: sourceControlScope,
                            onSwitchBranch: (branch) async {
                              await controller.switchWorkspaceBranch(
                                project: project!,
                                workspace: workspace,
                                branch: branch,
                              );
                              await ref
                                  .read(
                                    workspaceSourceControlControllerProvider(
                                      sourceControlScope.path,
                                    ).notifier,
                                  )
                                  .refresh();
                            },
                          ),
                        const LanguageIntelligenceStatusBarControl(),
                        const ResourceStatusBarControl(),
                        const KeepAliveStatusBarControl(),
                        const RuntimeHostStatusBarControl(),
                      ],
                    ),
                  ),
                ],
              );
            },
          ),
        ),
      ),
    );
    return content;
  }

  Future<void> _openWorkspaceFile({
    required Workspace workspace,
    required String relativePath,
    bool preview = false,
    CodeOpenTarget? targetOverride,
  }) async {
    await ref
        .read(workspaceFileOpenCoordinatorProvider)
        .open<WorkspaceTabRecord>(
          workspace: workspace,
          relativePath: relativePath,
          preview: preview,
          targetOverride: targetOverride,
          openInAlera:
              ({required workspace, required relativePath, required preview}) =>
                  ref
                      .read(workbenchControllerProvider.notifier)
                      .openFileTab(
                        workspace: workspace,
                        relativePath: relativePath,
                        preview: preview,
                      ),
        );
  }

  Future<bool> _confirmCloseTabs(
    List<WorkspaceTabRecord> tabs,
    List<String> tabIds,
  ) {
    return confirmCloseWorkspaceTabs(context, ref, tabs, tabIds);
  }

  void _showError(String message) {
    AleraToast.show(context, message: message, tone: .error);
  }
}

class const WorkspaceBranchStatusBarControl({
  super.key,
  required final Workspace workspace,
  required final WorkspaceSourceControlScope sourceControlScope,
  required final Future<void> Function(String branch) onSwitchBranch,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<WorkspaceBranchStatusBarControl> createState() =>
      _WorkspaceBranchStatusBarControlState();
}

class _WorkspaceBranchStatusBarControlState
    extends ConsumerState<WorkspaceBranchStatusBarControl> {
  final MenuController _menu = MenuController();
  final TextEditingController _search = TextEditingController();
  final FocusNode _searchFocus = FocusNode(
    debugLabel: 'workspace-branch-search',
  );
  List<String> _branches = const <String>[];
  String? _current;
  String _query = '';
  bool _loading = false;
  bool _switching = false;
  int _generation = 0;

  @override
  void initState() {
    super.initState();
    _current = _normalizedWorkspaceBranch(widget.workspace.branch);
    unawaited(_loadBranches());
  }

  @override
  void didUpdateWidget(covariant WorkspaceBranchStatusBarControl oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.sourceControlScope.path != widget.sourceControlScope.path) {
      _current = _normalizedWorkspaceBranch(widget.workspace.branch);
      _branches = const <String>[];
      unawaited(_loadBranches());
      return;
    }
    if (oldWidget.workspace.branch != widget.workspace.branch) {
      _current =
          _normalizedWorkspaceBranch(widget.workspace.branch) ?? _current;
    }
  }

  @override
  void dispose() {
    _generation += 1;
    _search.dispose();
    _searchFocus.dispose();
    super.dispose();
  }

  Future<void> _loadBranches() async {
    final generation = ++_generation;
    if (mounted) setState(() => _loading = true);
    try {
      final backend = ref.read(gitBackendProvider);
      final path = widget.sourceControlScope.path;
      final current = await backend.currentBranch(path);
      final branches = await backend.listBranches(path);
      var remotes = const <GitRemote>[];
      try {
        remotes = await backend.listRemotes(path);
      } on Object {
        // Keep local branch discovery usable when remote metadata is unavailable.
      }
      if (!mounted || generation != _generation) return;
      final remotePrefixes = <String>[
        for (final remote in remotes)
          if (remote.name.trim().isNotEmpty) '${remote.name.trim()}/',
      ];
      final localBranches = <String>{
        for (final branch in branches)
          if (!remotePrefixes.any(branch.startsWith)) branch,
        if (current != 'HEAD' && current.trim().isNotEmpty) current,
      }.toList()..sort();
      setState(() {
        _branches = localBranches;
        _current = _normalizedWorkspaceBranch(current) ?? _current;
        _loading = false;
      });
    } on Object {
      if (mounted && generation == _generation) {
        setState(() => _loading = false);
      }
    }
  }

  void _openMenu() {
    if (_switching) return;
    _search.clear();
    if (_query.isNotEmpty) setState(() => _query = '');
    _menu.open();
    unawaited(_loadBranches());
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _searchFocus.requestFocus();
    });
  }

  void _selectBranch(String branch) {
    _menu.close();
    if (_switching || branch == _current) return;
    unawaited(_switchBranch(branch));
  }

  Future<void> _switchBranch(String branch) async {
    setState(() => _switching = true);
    try {
      await widget.onSwitchBranch(branch);
      if (mounted) setState(() => _current = branch);
    } catch (error) {
      if (mounted) {
        AleraToast.show(context, message: error.toString(), tone: .error);
      }
    } finally {
      if (mounted) setState(() => _switching = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final label =
        _current ??
        _normalizedWorkspaceBranch(widget.workspace.branch) ??
        'HEAD';
    final query = _query.trim().toLowerCase();
    final filtered = _branches
        .where(
          (branch) => query.isEmpty || branch.toLowerCase().contains(query),
        )
        .toList(growable: false);
    final rows = buildWorkspaceBranchTreeRows(filtered);

    return MenuAnchor(
      controller: _menu,
      menuChildren: <Widget>[
        SizedBox(
          width: 320,
          height: 360,
          child: Padding(
            padding: const EdgeInsets.all(AleraTokens.space8),
            child: Column(
              children: <Widget>[
                TextField(
                  key: const ValueKey<String>('workspace-branch-search'),
                  controller: _search,
                  focusNode: _searchFocus,
                  textInputAction: .search,
                  decoration: InputDecoration(
                    isDense: true,
                    hintText: context.tr('Search branches'),
                    prefixIcon: const Icon(Icons.search, size: 16),
                    border: const OutlineInputBorder(),
                  ),
                  onChanged: (value) => setState(() => _query = value),
                ),
                const SizedBox(height: AleraTokens.space8),
                Expanded(
                  child: _loading && rows.isEmpty
                      ? const Center(
                          child: SizedBox(
                            width: 18,
                            height: 18,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          ),
                        )
                      : rows.isEmpty
                      ? Center(child: Text(context.tr('No matching branches')))
                      : ListView.builder(
                          primary: false,
                          padding: EdgeInsets.zero,
                          itemCount: rows.length,
                          itemBuilder: (context, index) {
                            final row = rows[index];
                            final selected = row.branch == _current;
                            return MenuItemButton(
                              key: row.branch == null
                                  ? null
                                  : ValueKey<String>(
                                      'workspace-branch-option:${row.branch}',
                                    ),
                              onPressed: row.branch == null
                                  ? null
                                  : () => _selectBranch(row.branch!),
                              child: Padding(
                                padding: EdgeInsets.only(
                                  left: row.depth * AleraTokens.space12,
                                ),
                                child: Row(
                                  children: <Widget>[
                                    SizedBox(
                                      width: 16,
                                      child: selected
                                          ? const Icon(
                                              AleraIcons.check,
                                              size: 14,
                                            )
                                          : row.hasChildren
                                          ? const Icon(
                                              AleraIcons.chevronRight,
                                              size: 12,
                                              color:
                                                  AleraTokens.foregroundFaint,
                                            )
                                          : null,
                                    ),
                                    const SizedBox(width: AleraTokens.space6),
                                    Expanded(
                                      child: Tooltip(
                                        message: row.fullPath,
                                        child: Text(
                                          row.segment,
                                          maxLines: 1,
                                          overflow: .ellipsis,
                                        ),
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            );
                          },
                        ),
                ),
              ],
            ),
          ),
        ),
      ],
      builder: (context, controller, child) => Material(
        color: Colors.transparent,
        child: InkWell(
          key: const ValueKey<String>('workspace-branch-status'),
          onTap: _switching
              ? null
              : controller.isOpen
              ? controller.close
              : _openMenu,
          mouseCursor: WidgetStateMouseCursor.clickable,
          child: Tooltip(
            message: '${context.tr('Current Branch')}: $label',
            child: Container(
              height: AleraTokens.statusBarHeight,
              constraints: const BoxConstraints(maxWidth: 220),
              padding: const EdgeInsets.symmetric(
                horizontal: AleraTokens.space8,
              ),
              decoration: const BoxDecoration(
                border: Border(
                  left: BorderSide(color: AleraTokens.borderSubtle),
                ),
              ),
              child: Row(
                mainAxisSize: .min,
                children: <Widget>[
                  const Icon(
                    AleraIcons.gitBranch,
                    size: 13,
                    color: AleraTokens.foregroundMuted,
                  ),
                  const SizedBox(width: AleraTokens.space6),
                  Flexible(
                    child: Text(
                      label,
                      maxLines: 1,
                      overflow: .ellipsis,
                      style: AleraTokens.monoStyle.copyWith(fontSize: 10),
                    ),
                  ),
                  const SizedBox(width: AleraTokens.space4),
                  if (_switching || _loading)
                    const SizedBox(
                      width: 10,
                      height: 10,
                      child: CircularProgressIndicator(strokeWidth: 1.5),
                    )
                  else
                    const Icon(
                      AleraIcons.chevronDown,
                      size: 12,
                      color: AleraTokens.foregroundMuted,
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

@visibleForTesting
List<WorkspaceBranchTreeRow> buildWorkspaceBranchTreeRows(
  List<String> branches,
) {
  final roots = <String, _WorkspaceBranchTreeNode>{};
  for (final raw in branches) {
    final branch = raw.trim();
    if (branch.isEmpty) continue;
    final segments = branch
        .split('/')
        .where((segment) => segment.isNotEmpty)
        .toList(growable: false);
    var nodes = roots;
    var path = '';
    _WorkspaceBranchTreeNode? node;
    for (final segment in segments) {
      path = path.isEmpty ? segment : '$path/$segment';
      node = nodes.putIfAbsent(
        segment,
        () => _WorkspaceBranchTreeNode(segment: segment, fullPath: path),
      );
      nodes = node.children;
    }
    node?.branch = branch;
  }

  final rows = <WorkspaceBranchTreeRow>[];
  void append(Map<String, _WorkspaceBranchTreeNode> nodes, int depth) {
    final sorted = nodes.values.toList()
      ..sort((a, b) => a.segment.compareTo(b.segment));
    for (final node in sorted) {
      rows.add(
        WorkspaceBranchTreeRow(
          segment: node.segment,
          fullPath: node.fullPath,
          branch: node.branch,
          depth: depth,
          hasChildren: node.children.isNotEmpty,
        ),
      );
      append(node.children, depth + 1);
    }
  }

  append(roots, 0);
  return rows;
}

@visibleForTesting
class const WorkspaceBranchTreeRow({
  required this.segment,
  required this.fullPath,
  required this.branch,
  required this.depth,
  required this.hasChildren,
}) {
  final String segment;
  final String fullPath;
  final String? branch;
  final int depth;
  final bool hasChildren;
}

final class _WorkspaceBranchTreeNode {
  _WorkspaceBranchTreeNode({required this.segment, required this.fullPath});

  final String segment;
  final String fullPath;
  final Map<String, _WorkspaceBranchTreeNode> children =
      <String, _WorkspaceBranchTreeNode>{};
  String? branch;
}

String? _normalizedWorkspaceBranch(String? value) {
  final branch = value?.trim();
  return branch == null || branch.isEmpty || branch == 'HEAD' ? null : branch;
}
