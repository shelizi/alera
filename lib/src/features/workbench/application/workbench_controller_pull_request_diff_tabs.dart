part of 'workbench_controller.dart';

mixin _WorkbenchControllerPullRequestDiffTabs
    on _$WorkbenchController, _WorkbenchControllerInternals {
  Future<WorkspaceTabRecord> openGitPullRequestDiffTab({
    required Workspace workspace,
    String? gitDiffRoot,
    required int pullRequestNumber,
    required String commitOid,
    required String parentOid,
    required String retentionId,
    String? subject,
    String? targetGroupId,
  }) async {
    try {
      final previousTabs = state.tabsFor(workspace.id);
      final layout = _layoutForMutation(workspace.id, previousTabs);
      final tab =
          await WorkbenchPullRequestDiffTabOpenCoordinator(
            tabStore: WorkbenchPullRequestDiffTabStoreAdapter(
              _workspaceTabService,
            ),
            hostedReviewRetention: _hostedReviewRetention,
          ).open(
            workspace: workspace,
            previousTabs: previousTabs,
            gitDiffRoot: gitDiffRoot,
            pullRequestNumber: pullRequestNumber,
            commitOid: commitOid,
            parentOid: parentOid,
            retentionId: retentionId,
            subject: subject,
            onReady: ({required tab, required alreadyOpen}) async {
              final tabs = alreadyOpen
                  ? previousTabs
                  : <WorkspaceTabRecord>[...previousTabs, tab];
              _setTabsForWorkspace(workspace.id, tabs);
              final groupId = targetGroupId ?? layout.activeGroupId;
              final nextLayout = alreadyOpen
                  ? layout.setActiveTab(
                      groupId: layout.groupIdForTab(tab.id) ?? groupId,
                      tabId: tab.id,
                    )
                  : layout.addTabToGroup(groupId: groupId, tabId: tab.id);
              await _applyLayout(nextLayout.sanitize(tabs), persist: true);
            },
          );
      state = state.copyWith(error: null);
      return tab;
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }
}
