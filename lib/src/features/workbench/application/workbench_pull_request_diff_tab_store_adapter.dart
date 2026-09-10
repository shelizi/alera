import 'package:alera/src/features/workbench/application/workbench_pull_request_diff_tab_store.dart';
import 'package:alera/src/features/workbench/application/workspace_tab_service.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchPullRequestDiffTabStoreAdapter
    implements WorkbenchPullRequestDiffTabStore {
  const WorkbenchPullRequestDiffTabStoreAdapter(this._service);

  final WorkspaceTabService _service;

  @override
  Future<WorkspaceTabRecord> openOrCreateGitPullRequestDiffTab({
    required String workspaceId,
    String? gitDiffRoot,
    required int pullRequestNumber,
    required String commitOid,
    required String parentOid,
    required String retentionId,
    String? subject,
  }) {
    return _service.openOrCreateGitPullRequestDiffTab(
      workspaceId: workspaceId,
      gitDiffRoot: gitDiffRoot,
      pullRequestNumber: pullRequestNumber,
      commitOid: commitOid,
      parentOid: parentOid,
      retentionId: retentionId,
      subject: subject,
    );
  }

  @override
  Future<void> closeTab(String tabId) => _service.closeTab(tabId);
}
