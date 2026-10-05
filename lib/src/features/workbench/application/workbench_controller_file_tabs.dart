part of 'workbench_controller.dart';

/// Opening and pinning file-backed tabs, including shared preview replacement.
mixin _WorkbenchControllerFileTabs
    on _$WorkbenchController, _WorkbenchControllerInternals {
  Future<WorkspaceTabRecord> openEditorTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
    bool preview = false,
  }) => _tabLayoutOwner.openEditorTab(
    workspace: workspace,
    relativePath: relativePath,
    targetGroupId: targetGroupId,
    preview: preview,
  );

  Future<WorkspaceTabRecord> openMarkdownViewerTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
    bool preview = false,
  }) => _tabLayoutOwner.openMarkdownViewerTab(
    workspace: workspace,
    relativePath: relativePath,
    targetGroupId: targetGroupId,
    preview: preview,
  );

  Future<WorkspaceTabRecord> openPdfTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
    bool preview = false,
  }) => _tabLayoutOwner.openPdfTab(
    workspace: workspace,
    relativePath: relativePath,
    targetGroupId: targetGroupId,
    preview: preview,
  );

  Future<WorkspaceTabRecord> openFileTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
    bool preview = false,
  }) => _tabLayoutOwner.openFileTab(
    workspace: workspace,
    relativePath: relativePath,
    targetGroupId: targetGroupId,
    preview: preview,
  );

  Future<WorkspaceTabRecord> openGitDiffTab({
    required Workspace workspace,
    String? relativePath,
    GitChangeArea? area,
    required WorkspaceGitDiffScope scope,
    String? gitDiffRoot,
    String? targetGroupId,
    bool preview = false,
  }) => _tabLayoutOwner.openGitDiffTab(
    workspace: workspace,
    relativePath: relativePath,
    area: area,
    scope: scope,
    gitDiffRoot: gitDiffRoot,
    targetGroupId: targetGroupId,
    preview: preview,
  );

  Future<WorkspaceTabRecord> openGitFileRevisionDiffTab({
    required Workspace workspace,
    required String relativePath,
    String? gitDiffRoot,
    required String revisionOid,
    required String compareRef,
    String? subject,
    String? targetGroupId,
    bool preview = false,
  }) => _tabLayoutOwner.openGitFileRevisionDiffTab(
    workspace: workspace,
    relativePath: relativePath,
    gitDiffRoot: gitDiffRoot,
    revisionOid: revisionOid,
    compareRef: compareRef,
    subject: subject,
    targetGroupId: targetGroupId,
    preview: preview,
  );

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
  }) => _tabLayoutOwner.openGitCommitDiffTab(
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
    targetGroupId: targetGroupId,
    preview: preview,
  );

  Future<WorkspaceTabRecord> keepPreviewTab(String tabId) =>
      _tabLayoutOwner.keepPreviewTab(tabId);
}
