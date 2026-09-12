part of 'workbench_controller.dart';

/// Facade over the catalog owner: project and workspace catalog mutations
/// plus selection entry points that stay delegated to the selection owner.
mixin _WorkbenchControllerProjects
    on _$WorkbenchController, _WorkbenchControllerInternals {
  Future<List<String>> listSourceBranches(Project project) =>
      _catalogOwner.listSourceBranches(project);

  Future<void> reconcileProjectWorkspaces(String projectId) =>
      _catalogOwner.reconcileProjectWorkspaces(projectId);

  Future<Project> addLocalProject({required String path, String? name}) =>
      _catalogOwner.addLocalProject(path: path, name: name);

  Future<Project> cloneProject({
    required String gitUrl,
    required String destinationPath,
    String? name,
  }) => _catalogOwner.cloneProject(
    gitUrl: gitUrl,
    destinationPath: destinationPath,
    name: name,
  );

  Future<Project> addProject({required String repoPath, String? name}) =>
      _catalogOwner.addProject(repoPath: repoPath, name: name);

  Future<void> renameProject({
    required String projectId,
    required String name,
  }) => _catalogOwner.renameProject(projectId: projectId, name: name);

  Future<void> sleepWorkspace(Workspace workspace) =>
      _catalogOwner.sleepWorkspace(workspace);

  Future<void> removeProject(String projectId) =>
      _catalogOwner.removeProject(projectId);

  Future<void> deleteWorkspace({
    required Project project,
    required Workspace workspace,
    bool deleteBranch = true,
    String? activeWorkspaceId,
  }) => _catalogOwner.deleteWorkspace(
    project: project,
    workspace: workspace,
    deleteBranch: deleteBranch,
    activeWorkspaceId: activeWorkspaceId,
  );

  Future<void> renameWorkspace({
    required String workspaceId,
    required String name,
  }) => _catalogOwner.renameWorkspace(workspaceId: workspaceId, name: name);

  Future<void> setWorkspacePinned({
    required String workspaceId,
    required bool isPinned,
  }) => _catalogOwner.setWorkspacePinned(
    workspaceId: workspaceId,
    isPinned: isPinned,
  );

  /// Pins or unpins [workspaceId] and every descendant of [workspaceId].
  /// No-ops workspaces that already match [isPinned].
  Future<void> setWorkspaceTreePinned({
    required String workspaceId,
    required bool isPinned,
  }) => _catalogOwner.setWorkspaceTreePinned(
    workspaceId: workspaceId,
    isPinned: isPinned,
  );

  Future<List<WorkspaceTag>> listWorkspaceTags() =>
      _catalogOwner.listWorkspaceTags();

  Future<List<WorkspaceRelation>> listWorkspaceRelations() =>
      _catalogOwner.listWorkspaceRelations();

  Future<WorkspaceTag> createWorkspaceTag(String name) =>
      _catalogOwner.createWorkspaceTag(name);

  Future<void> deleteWorkspaceTag(String tagId) =>
      _catalogOwner.deleteWorkspaceTag(tagId);

  Future<void> updateWorkspaceTags({
    required Workspace workspace,
    required Set<String> tagIds,
  }) => _catalogOwner.updateWorkspaceTags(workspace: workspace, tagIds: tagIds);

  Future<void> setWorkspaceParent({
    required Workspace workspace,
    String? parentWorkspaceId,
  }) => _catalogOwner.setWorkspaceParent(
    workspace: workspace,
    parentWorkspaceId: parentWorkspaceId,
  );

  Future<void> selectWorkspace({
    required Project project,
    required Workspace workspace,
  }) {
    return _selectionOwner.selectWorkspace(
      project: project,
      workspace: workspace,
      ensureInitialTerminal: true,
    );
  }

  Future<void> activateProject(Project project) async {
    _selectionOwner.activateProject(project);
  }
}
