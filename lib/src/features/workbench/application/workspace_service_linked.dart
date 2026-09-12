part of 'workspace_service.dart';

/// Linked-worktree provisioning: branch validation against the source repo,
/// managed-runtime delegation when a host owns the lifecycle, and the
/// worktree + setup-report path for local creation.
extension WorkspaceServiceLinkedOps on WorkspaceService {
  Future<WorkspaceCreationResult> createLinkedWorkspace({
    required Project project,
    required String sourceBranch,
    required String newBranchName,
    bool reuseExistingBranch = false,
    String? name,
  }) async {
    if (!project.supportsLinkedWorkspaces) {
      throw WorkspaceException(
        'Linked workspaces require a Git repository project',
      );
    }
    final normalizedSource = sourceBranch.trim();
    final normalizedBranch = newBranchName.trim();
    if (!reuseExistingBranch && normalizedSource.isEmpty) {
      throw WorkspaceException('Source branch is required');
    }
    if (normalizedBranch.isEmpty) {
      throw WorkspaceException('New branch name is required');
    }

    final managedRuntime = _managedRuntime;
    if (managedRuntime != null) {
      return managedRuntime.createLinkedWorkspace(
        project: project,
        sourceBranch: normalizedSource,
        newBranchName: normalizedBranch,
        reuseExistingBranch: reuseExistingBranch,
        name: name,
      );
    }

    await _validateBranchName(normalizedBranch);
    if (reuseExistingBranch) {
      await _ensureTargetBranchExists(project, normalizedBranch);
    } else {
      await _ensureSourceBranchExists(project, normalizedSource);
      await _ensureNewBranchDoesNotExist(project, normalizedBranch);
    }

    final workspaces = await _repository.listWorkspaces(project.id);
    if (workspaces.any(
      (workspace) => workspace.isActive && workspace.branch == normalizedBranch,
    )) {
      throw WorkspaceException(
        'A workspace for branch "$normalizedBranch" already exists',
      );
    }

    final displayName = (name ?? normalizedBranch).trim();
    final pathSlug = _slugifyPathSegment(displayName);
    final workspacePath = _resolveWorkspacePath(project, pathSlug);
    if (workspaces.any(
      (workspace) =>
          workspace.isActive && p.equals(workspace.path, workspacePath),
    )) {
      throw WorkspaceException(
        'A workspace already exists at "$workspacePath"',
      );
    }

    if (!reuseExistingBranch) {
      try {
        await _gitBackend.refreshSourceBranch(
          repoPath: project.repoPath,
          sourceBranch: normalizedSource,
        );
      } on GitException {
        // Source branch refresh against upstream is best-effort.
        // If the branch has diverged, the host is offline, or pull cannot
        // fast-forward, continue creating the worktree from the local branch.
      }
    }

    final parent = Directory(p.dirname(workspacePath));
    if (!parent.existsSync()) {
      parent.createSync(recursive: true);
    }

    try {
      await _gitBackend.createWorktree(
        repoPath: project.repoPath,
        targetBranch: normalizedBranch,
        path: workspacePath,
        sourceBranch: normalizedSource,
        reuseExistingBranch: reuseExistingBranch,
      );
    } on GitException catch (error) {
      throw WorkspaceException(
        'git worktree add failed',
        stderr: error.context,
      );
    }

    final workspace = Workspace(
      id: _uuid.v4(),
      projectId: project.id,
      name: displayName,
      branch: normalizedBranch,
      path: workspacePath,
      createdAt: _now(),
      updatedAt: _now(),
      kind: .linked,
      status: .active,
      sourceBranch: reuseExistingBranch ? null : normalizedSource,
      reusesExistingBranch: reuseExistingBranch,
    );
    await _repository.upsertWorkspace(workspace);
    final setupReport = await _runWorktreeSetup(
      project: project,
      workspace: workspace,
    );
    return WorkspaceCreationResult(
      workspace: workspace,
      setupReport: setupReport,
    );
  }

  Future<WorktreeSetupReport> _runWorktreeSetup({
    required Project project,
    required Workspace workspace,
  }) async {
    final effective = await _projectConfigReader.resolve(project);
    final error = effective.error;
    if (error != null) {
      return WorktreeSetupReport(
        steps: <WorktreeSetupStepReport>[
          WorktreeSetupStepReport(
            kind: .config,
            label: 'alera.toml',
            succeeded: false,
            message: error.toString(),
          ),
        ],
      );
    }
    return _worktreeSetupRunner.run(
      project: project,
      workspace: workspace,
      config: effective.config,
    );
  }

  Future<void> _ensureSourceBranchExists(Project project, String branch) async {
    final branches = await _projectService.listGitBranches(project.repoPath);
    if (!branches.contains(branch)) {
      throw WorkspaceException('Source branch "$branch" does not exist');
    }
  }

  Future<void> _ensureNewBranchDoesNotExist(
    Project project,
    String branchName,
  ) async {
    final exists = await _gitBackend.branchExists(project.repoPath, branchName);
    if (exists) {
      throw WorkspaceException('Branch "$branchName" already exists');
    }
  }

  String _resolveWorkspacePath(Project project, String slug) {
    final projectSlug = _slugifyPathSegment(p.basename(project.repoPath));
    return p.join(_workspaceRoot.resolve(), '$projectSlug-${project.id}', slug);
  }

  String _slugifyPathSegment(String input) {
    final normalized = input
        .trim()
        .toLowerCase()
        .replaceAll(RegExp(r'[\s_/]+'), '-')
        .replaceAll(RegExp(r'[^a-z0-9-]'), '-')
        .replaceAll(RegExp(r'-+'), '-')
        .replaceAll(RegExp(r'^-|-$'), '');
    if (normalized.isEmpty) {
      throw WorkspaceException('Workspace name must contain a letter or digit');
    }
    return normalized;
  }
}
