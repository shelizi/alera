import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

abstract interface class WorkbenchPullRequestDiffTabStore {
  Future<WorkspaceTabRecord> openOrCreateGitPullRequestDiffTab({
    required String workspaceId,
    String? gitDiffRoot,
    required int pullRequestNumber,
    required String commitOid,
    required String parentOid,
    required String retentionId,
    String? subject,
  });

  Future<void> closeTab(String tabId);
}
