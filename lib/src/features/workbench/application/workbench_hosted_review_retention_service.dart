import 'dart:async';

import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/shared/infra/git/git_hosted_review_retention_port.dart';

abstract interface class WorkbenchHostedReviewTabRetention {
  Future<void> releaseTab(
    Workspace workspace,
    WorkspaceTabRecord tab, {
    String? fallbackWorkspacePath,
  });
}

abstract interface class WorkbenchHostedReviewRangeRetention {
  Future<void> persist({
    required Workspace workspace,
    required String? relativeRoot,
    required String retentionId,
  });

  Future<void> release({
    required Workspace workspace,
    required String? relativeRoot,
    required String retentionId,
    String? fallbackWorkspacePath,
  });
}

final class WorkbenchHostedReviewRetentionService
    implements
        WorkbenchHostedReviewTabRetention,
        WorkbenchHostedReviewRangeRetention {
  // Retain existing named constructor parameter; inject only the narrow port.
  // ignore: prefer_initializing_formals
  const WorkbenchHostedReviewRetentionService({
    required GitHostedReviewRetentionPort gitBackend,
  }) : _gitBackend = gitBackend;

  final GitHostedReviewRetentionPort _gitBackend;

  @override
  Future<void> persist({
    required Workspace workspace,
    required String? relativeRoot,
    required String retentionId,
  }) {
    return _gitBackend.persistHostedReviewRange(
      path: _reviewPath(workspace.path, relativeRoot),
      retentionId: retentionId,
    );
  }

  @override
  Future<void> release({
    required Workspace workspace,
    required String? relativeRoot,
    required String retentionId,
    String? fallbackWorkspacePath,
  }) async {
    final path = _reviewPath(workspace.path, relativeRoot);
    try {
      await _gitBackend.releaseHostedReviewRange(
        path: path,
        retentionId: retentionId,
      );
    } catch (_) {
      if (fallbackWorkspacePath == null ||
          fallbackWorkspacePath == workspace.path) {
        return;
      }
      final fallbackPath = _reviewPath(fallbackWorkspacePath, relativeRoot);
      try {
        await _gitBackend.releaseHostedReviewRange(
          path: fallbackPath,
          retentionId: retentionId,
        );
      } catch (_) {
        // A stale retention ref must never make a persisted tab impossible to close.
      }
    }
  }

  @override
  Future<void> releaseTab(
    Workspace workspace,
    WorkspaceTabRecord tab, {
    String? fallbackWorkspacePath,
  }) async {
    final retentionId = tab.gitDiffHostedReviewRetentionId;
    if (tab.gitDiffSource != WorkspaceGitDiffSource.pullRequest ||
        retentionId == null) {
      return;
    }
    await release(
      workspace: workspace,
      relativeRoot: tab.gitDiffRoot,
      retentionId: retentionId,
      fallbackWorkspacePath: fallbackWorkspacePath,
    );
  }

  void releaseTabsInBackground(
    Workspace workspace,
    Iterable<WorkspaceTabRecord> tabs,
  ) {
    for (final tab in tabs) {
      unawaited(releaseTab(workspace, tab));
    }
  }

  String _reviewPath(String workspacePath, String? relativeRoot) {
    return relativeRoot == null
        ? workspacePath
        : sourceControlRootAbsolutePath(
            workspacePath: workspacePath,
            relativeRoot: relativeRoot,
          );
  }
}
