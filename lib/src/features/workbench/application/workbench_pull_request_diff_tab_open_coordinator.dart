import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_pull_request_diff_tab_store.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchPullRequestDiffTabReady = Future<void> Function({
  required WorkspaceTabRecord tab,
  required bool alreadyOpen,
});

final class WorkbenchPullRequestDiffTabOpenCoordinator {
  const WorkbenchPullRequestDiffTabOpenCoordinator({
    required WorkbenchPullRequestDiffTabStore tabStore,
    required WorkbenchHostedReviewRangeRetention hostedReviewRetention,
  }) : _tabStore = tabStore,
       _hostedReviewRetention = hostedReviewRetention;

  final WorkbenchPullRequestDiffTabStore _tabStore;
  final WorkbenchHostedReviewRangeRetention _hostedReviewRetention;

  Future<WorkspaceTabRecord> open({
    required Workspace workspace,
    required List<WorkspaceTabRecord> previousTabs,
    String? gitDiffRoot,
    required int pullRequestNumber,
    required String commitOid,
    required String parentOid,
    required String retentionId,
    String? subject,
    required WorkbenchPullRequestDiffTabReady onReady,
  }) async {
    var retainedByTab = false;
    WorkspaceTabRecord? newTab;
    try {
      final tab = await _tabStore.openOrCreateGitPullRequestDiffTab(
        workspaceId: workspace.id,
        gitDiffRoot: gitDiffRoot,
        pullRequestNumber: pullRequestNumber,
        commitOid: commitOid,
        parentOid: parentOid,
        retentionId: retentionId,
        subject: subject,
      );
      final alreadyOpen = previousTabs.any(
        (candidate) => candidate.id == tab.id,
      );
      if (!alreadyOpen) {
        newTab = tab;
      }

      if (tab.gitDiffHostedReviewRetentionId == retentionId) {
        await _hostedReviewRetention.persist(
          workspace: workspace,
          relativeRoot: gitDiffRoot,
          retentionId: retentionId,
        );
        retainedByTab = true;
      } else {
        await _hostedReviewRetention.release(
          workspace: workspace,
          relativeRoot: gitDiffRoot,
          retentionId: retentionId,
        );
      }

      await onReady(tab: tab, alreadyOpen: alreadyOpen);
      return tab;
    } catch (_) {
      if (!retainedByTab) {
        if (newTab case final tab?) {
          await _tabStore.closeTab(tab.id);
        }
        await _hostedReviewRetention.release(
          workspace: workspace,
          relativeRoot: gitDiffRoot,
          retentionId: retentionId,
        );
      }
      rethrow;
    }
  }
}
