part of 'workbench_tab_layout_owner.dart';

/// Opening and pinning file-backed tabs, including shared preview replacement.
extension WorkbenchTabLayoutOwnerFileTabs on WorkbenchTabLayoutOwner {
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
            return _host.workspaceTabService.openOrCreateEditorTab(
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
            return _host.workspaceTabService.openOrCreateMarkdownViewerTab(
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
            return _host.workspaceTabService.openOrCreatePdfTab(
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
            return _host.workspaceTabService.openOrCreateGitDiffTab(
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

  Future<WorkspaceTabRecord> openGitFileRevisionDiffTab({
    required Workspace workspace,
    required String relativePath,
    String? gitDiffRoot,
    required String revisionOid,
    required String compareRef,
    String? subject,
    String? targetGroupId,
    bool preview = false,
  }) {
    return _openReplaceableTab(
      workspace: workspace,
      targetGroupId: targetGroupId,
      preview: preview,
      createTab:
          ({required workspaceId, required preview, replacePreviewTabId}) {
            return _host.workspaceTabService.openOrCreateGitFileRevisionDiffTab(
              workspaceId: workspaceId,
              relativePath: relativePath,
              gitDiffRoot: gitDiffRoot,
              revisionOid: revisionOid,
              compareRef: compareRef,
              subject: subject,
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
            return _host.workspaceTabService.openOrCreateGitCommitDiffTab(
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
      final tab = await _host.workspaceTabService.keepPreviewTab(tabId);
      _host.emitState(
        applyWorkbenchTabUpdateState(
          state: _host.readState(),
          tab: tab,
        ).copyWith(error: null),
      );
      return tab;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
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
      final previousTabs = _host.readState().tabsFor(workspace.id);
      final layout = _layoutForMutation(workspace.id, previousTabs);
      final groupId = targetGroupId ?? layout.activeGroupId;
      final result =
          await WorkbenchReplaceableTabOpenCoordinator(
            editorSessions: _host.replaceableTabEditorSessions,
          ).open(
            workspaceId: workspace.id,
            previousTabs: previousTabs,
            layout: layout,
            targetGroupId: groupId,
            preview: preview,
            keepPreviewTab: _host.workspaceTabService.keepPreviewTab,
            onPreviewPinned: (tab) {
              _host.emitState(
                applyWorkbenchTabUpdateState(
                  state: _host.readState(),
                  tab: tab,
                ).copyWith(error: null),
              );
              return _host.readState().tabsFor(workspace.id);
            },
            createTab: createTab,
            applyTabs: (tabs) => applyWorkspaceTabs(workspace.id, tabs),
            applyLayout: (layout) =>
                applyWorkspaceLayout(layout, persist: true),
          );
      _host.emitState(_host.readState().copyWith(error: null));
      return result.tab;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }
}
