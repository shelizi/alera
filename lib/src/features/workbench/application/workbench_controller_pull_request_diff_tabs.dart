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
  }) => _tabLayoutOwner.openGitPullRequestDiffTab(
    workspace: workspace,
    gitDiffRoot: gitDiffRoot,
    pullRequestNumber: pullRequestNumber,
    commitOid: commitOid,
    parentOid: parentOid,
    retentionId: retentionId,
    subject: subject,
    targetGroupId: targetGroupId,
  );
}
