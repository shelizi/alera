part of 'alera_shell_page.dart';

class _AleraShellPageBodyState extends ConsumerState<_AleraShellPageBody>
    with WidgetsBindingObserver {
  String? _lastErrorMessage;
  late final WorkbenchGitRepositoryLifecycleMonitor _gitRepositoryMonitor;

  @override
  void initState() {
    super.initState();
    _gitRepositoryMonitor = WorkbenchGitRepositoryLifecycleMonitor(
      refreshProject: (projectId) async {
        await ref
            .read(workbenchControllerProvider.notifier)
            .refreshProjectGitRepository(projectId);
      },
      onError: (error) =>
          Logger('AleraGitRepositoryDetection')
              .fine('Automatic Git repository re-detection failed: $error'),
    );
    WidgetsBinding.instance.addObserver(this);
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      final controller = ref.read(workbenchControllerProvider.notifier);
      await controller.bootstrap();
      if (!mounted) return;
      _gitRepositoryMonitor.selectProject(
        ref.read(workbenchControllerProvider).activeProject,
      );

      // Test-only navigation: the Windows E2E runner keeps its SQLite profile
      // separate from the installed app, but a new process normally starts on
      // Welcome without a selected workspace. Automated PTY handoff tests
      // must not click an unrelated window on a mixed-DPI remote desktop.
      // Both independent compile-time gates must be enabled explicitly.
      const autoSelectE2ETerminal =
          String.fromEnvironment('ALERA_E2E_DESKTOP') == '1' &&
          bool.fromEnvironment('ALERA_E2E_AUTO_SELECT_TERMINAL');
      if (autoSelectE2ETerminal) {
        const workspaceId = 'alera-e2e-shared-window-workspace';
        const tabId = 'alera-e2e-shared-window-editor';
        // The project/workspace watchers hydrate asynchronously *after*
        // bootstrap() returns. Poll only this isolated test run, with a firm
        // deadline rather than blindly assuming the first snapshot is ready.
        var available = false;
        for (var attempt = 0; attempt < 100 && mounted; attempt++) {
          available = ref
              .read(workbenchControllerProvider)
              .workspacesByProject
              .values
              .expand((workspaces) => workspaces)
              .any((workspace) => workspace.id == workspaceId);
          if (available) break;
          await Future<void>.delayed(const Duration(milliseconds: 150));
        }
        if (!mounted) return;
        if (!available) {
          final state = ref.read(workbenchControllerProvider);
          Logger('AleraE2ETerminalBootstrap').warning(
            'E2E workspace was not hydrated after bootstrap '
            'projects=${state.projects.length} '
            'workspace_groups=${state.workspacesByProject.length} '
            'error_present=${state.error != null}',
          );
        } else {
          await controller.selectWorkspaceTab(
            workspaceId: workspaceId,
            tabId: tabId,
          );
          Logger('AleraE2ETerminalBootstrap')
              .info('E2E test workspace selected');
        }
      }
    });
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _gitRepositoryMonitor.onAppResumed();
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _gitRepositoryMonitor.dispose();
    super.dispose();
  }

  Future<void> _refreshGitRepository(
    Project project, {
    bool announceResult = true,
  }) async {
    try {
      final result = await ref
          .read(workbenchControllerProvider.notifier)
          .refreshProjectGitRepository(project.id);
      if (!mounted || !announceResult) return;
      AleraToast.show(
        context,
        message: result.isGitRepository
            ? 'Git repository detected'
            : 'No Git repository found',
      );
    } catch (error) {
      if (mounted) {
        AleraToast.show(
          context,
          message: 'Git repository detection failed: $error',
          tone: AleraToastTone.error,
        );
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    ref.listen<Project?>(
      workbenchControllerProvider.select((state) => state.activeProject),
      (_, project) => _gitRepositoryMonitor.selectProject(project),
    );
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
    ref.watch(workbenchGitAutoFetchCoordinatorProvider);
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
                            onRefreshRepository: project == null
                                ? null
                                : () => _refreshGitRepository(
                                    project,
                                    announceResult: sourceControlScope == null,
                                  ),
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
