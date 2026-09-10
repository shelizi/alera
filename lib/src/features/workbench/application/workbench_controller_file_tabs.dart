part of 'workbench_controller.dart';

/// Opening and pinning file-backed tabs, including shared preview replacement.
mixin _WorkbenchControllerFileTabs
    on _$WorkbenchController, _WorkbenchControllerInternals {
  Future<WorkspaceTabRecord> openEditorTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
    bool preview = false,
  }) {
    return _openReplaceableTab(
      workspace: workspace,
      targetGroupId: targetGroupId,
      preview: preview,
      createTab:
          ({required workspaceId, required preview, replacePreviewTabId}) {
            return _workspaceTabService.openOrCreateEditorTab(
              workspaceId: workspaceId,
              relativePath: relativePath,
              preview: preview,
              replacePreviewTabId: replacePreviewTabId,
            );
          },
    );
  }

  Future<WorkspaceTabRecord> openMarkdownViewerTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
    bool preview = false,
  }) {
    return _openReplaceableTab(
      workspace: workspace,
      targetGroupId: targetGroupId,
      preview: preview,
      createTab:
          ({required workspaceId, required preview, replacePreviewTabId}) {
            return _workspaceTabService.openOrCreateMarkdownViewerTab(
              workspaceId: workspaceId,
              relativePath: relativePath,
              preview: preview,
              replacePreviewTabId: replacePreviewTabId,
            );
          },
    );
  }

  Future<WorkspaceTabRecord> openPdfTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
    bool preview = false,
  }) {
    return _openReplaceableTab(
      workspace: workspace,
      targetGroupId: targetGroupId,
      preview: preview,
      createTab:
          ({required workspaceId, required preview, replacePreviewTabId}) {
            return _workspaceTabService.openOrCreatePdfTab(
              workspaceId: workspaceId,
              relativePath: relativePath,
              preview: preview,
              replacePreviewTabId: replacePreviewTabId,
            );
          },
    );
  }

  Future<WorkspaceTabRecord> openFileTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
    bool preview = false,
  }) {
    if (isWorkspaceMarkdownFilePath(relativePath)) {
      return openMarkdownViewerTab(
        workspace: workspace,
        relativePath: relativePath,
        targetGroupId: targetGroupId,
        preview: preview,
      );
    }
    return isWorkspacePdfFilePath(relativePath)
        ? openPdfTab(
            workspace: workspace,
            relativePath: relativePath,
            targetGroupId: targetGroupId,
            preview: preview,
          )
        : openEditorTab(
            workspace: workspace,
            relativePath: relativePath,
            targetGroupId: targetGroupId,
            preview: preview,
          );
  }

  Future<WorkspaceTabRecord> openGitDiffTab({
    required Workspace workspace,
    String? relativePath,
    GitChangeArea? area,
    required WorkspaceGitDiffScope scope,
    String? gitDiffRoot,
    String? targetGroupId,
    bool preview = false,
  }) {
    return _openReplaceableTab(
      workspace: workspace,
      targetGroupId: targetGroupId,
      preview: preview,
      createTab:
          ({required workspaceId, required preview, replacePreviewTabId}) {
            return _workspaceTabService.openOrCreateGitDiffTab(
              workspaceId: workspaceId,
              relativePath: relativePath,
              area: area,
              scope: scope,
              gitDiffRoot: gitDiffRoot,
              preview: preview,
              replacePreviewTabId: replacePreviewTabId,
            );
          },
    );
  }

  Future<WorkspaceTabRecord> openGitCommitDiffTab({
    required Workspace workspace,
    String? relativePath,
    String? oldPath,
    required WorkspaceGitDiffScope scope,
    String? gitDiffRoot,
    required String commitOid,
    String? parentOid,
    required String compareRef,
    String? subject,
    String? message,
    String? targetGroupId,
    bool preview = false,
  }) {
    return _openReplaceableTab(
      workspace: workspace,
      targetGroupId: targetGroupId,
      preview: preview,
      createTab:
          ({required workspaceId, required preview, replacePreviewTabId}) {
            return _workspaceTabService.openOrCreateGitCommitDiffTab(
              workspaceId: workspaceId,
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
              replacePreviewTabId: replacePreviewTabId,
            );
          },
    );
  }

  Future<WorkspaceTabRecord> keepPreviewTab(String tabId) {
    return _fileTabMutations.run(() => _keepPreviewTabUnlocked(tabId));
  }

  Future<WorkspaceTabRecord> _openReplaceableTab({
    required Workspace workspace,
    String? targetGroupId,
    required bool preview,
    required Future<WorkspaceTabRecord> Function({
      required String workspaceId,
      required bool preview,
      String? replacePreviewTabId,
    })
    createTab,
  }) {
    return _fileTabMutations.run(
      () => _openReplaceableTabUnlocked(
        workspace: workspace,
        targetGroupId: targetGroupId,
        preview: preview,
        createTab: createTab,
      ),
    );
  }

  Future<WorkspaceTabRecord> _keepPreviewTabUnlocked(String tabId) async {
    try {
      final tab = await _workspaceTabService.keepPreviewTab(tabId);
      state = applyWorkbenchTabUpdateState(
        state: state,
        tab: tab,
      ).copyWith(error: null);
      return tab;
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<WorkspaceTabRecord> _openReplaceableTabUnlocked({
    required Workspace workspace,
    String? targetGroupId,
    required bool preview,
    required Future<WorkspaceTabRecord> Function({
      required String workspaceId,
      required bool preview,
      String? replacePreviewTabId,
    })
    createTab,
  }) async {
    try {
      final previousTabs = state.tabsFor(workspace.id);
      final layout = _layoutForMutation(workspace.id, previousTabs);
      final groupId = targetGroupId ?? layout.activeGroupId;
      final result =
          await WorkbenchReplaceableTabOpenCoordinator(
            editorSessions: _replaceableTabEditorSessions,
          ).open(
            workspaceId: workspace.id,
            previousTabs: previousTabs,
            layout: layout,
            targetGroupId: groupId,
            preview: preview,
            keepPreviewTab: _workspaceTabService.keepPreviewTab,
            onPreviewPinned: (tab) {
              state = applyWorkbenchTabUpdateState(
                state: state,
                tab: tab,
              ).copyWith(error: null);
              return state.tabsFor(workspace.id);
            },
            createTab: createTab,
          );
      _setTabsForWorkspace(workspace.id, result.plan.tabs);
      await _applyLayout(result.plan.layout, persist: true);
      state = state.copyWith(error: null);
      return result.tab;
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }
}
