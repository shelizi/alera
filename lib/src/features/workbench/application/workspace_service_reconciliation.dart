part of 'workspace_service.dart';

extension WorkspaceServiceReconciliation on WorkspaceService {
  Future<List<Workspace>> reconcile(Project project) async {
    final mainWorkspace = await ensureMainWorkspace(project);
    if (!project.supportsLinkedWorkspaces) {
      final workspaces = await _repository.listWorkspaces(project.id);
      var changed = false;
      for (final workspace in workspaces) {
        if (workspace.id == mainWorkspace.id) {
          continue;
        }
        await _repository.removeWorkspace(workspace.id, cascadeTabs: true);
        changed = true;
      }
      return changed ? _repository.listWorkspaces(project.id) : workspaces;
    }
    final liveWorktrees = await _listLiveWorktrees(project.repoPath);
    final workspaces = await _repository.listWorkspaces(project.id);
    // When `git worktree list` fails (null) or doesn't even report the main
    // worktree, the listing can't be trusted. Skip pruning so a transient git
    // failure never hard-deletes live workspaces.
    final mainPath = _canonicalPath(mainWorkspace.path);
    final canPrune =
        liveWorktrees != null && liveWorktrees.containsKey(mainPath);
    final trackedCanonicalPaths = <String>{mainPath};
    var changed = false;
    for (final workspace in workspaces) {
      if (workspace.id == mainWorkspace.id) {
        continue;
      }
      final canonical = _canonicalPath(workspace.path);
      final live = liveWorktrees?[canonical];
      if (live == null) {
        if (canPrune) {
          await _repository.removeWorkspace(workspace.id, cascadeTabs: true);
          changed = true;
        } else {
          trackedCanonicalPaths.add(canonical);
        }
        continue;
      }
      trackedCanonicalPaths.add(canonical);
      if (workspace.branch != live.branch || workspace.path != live.path) {
        await _repository.upsertWorkspace(
          workspace.copyWith(
            branch: live.branch,
            path: live.path,
            updatedAt: _now(),
          ),
        );
        changed = true;
      }
    }

    if (liveWorktrees != null) {
      for (final entry in liveWorktrees.entries) {
        final canonical = entry.key;
        final live = entry.value;
        if (trackedCanonicalPaths.contains(canonical)) {
          continue;
        }
        final branch = live.branch.trim();
        final hasNamedBranch = branch.isNotEmpty && branch != 'HEAD';
        final baseName = p.basename(live.path).trim();
        final displayName = hasNamedBranch
            ? branch
            : (baseName.isNotEmpty ? baseName : project.name);

        final newWorkspace = Workspace(
          id: _uuid.v4(),
          projectId: project.id,
          name: displayName,
          branch: branch.isNotEmpty ? branch : null,
          path: live.path,
          createdAt: _now(),
          updatedAt: _now(),
          kind: WorkspaceKind.linked,
          status: WorkspaceStatus.active,
          sourceBranch: null,
          reusesExistingBranch: true,
        );
        await _repository.upsertWorkspace(newWorkspace);
        changed = true;
        trackedCanonicalPaths.add(canonical);
      }
    }

    // No awaits occurred after the authoritative catalog read if there
    // were no mutations. Reuse that snapshot instead of another Host RPC.
    return changed ? _repository.listWorkspaces(project.id) : workspaces;
  }

  Future<Map<String, ({String path, String branch})>?> _listLiveWorktrees(
    String repoPath,
  ) async {
    final List<GitWorktreeEntry> liveEntries;
    try {
      liveEntries = await _gitBackend.listWorktrees(repoPath);
    } on GitException {
      return null;
    }
    final entries = <String, ({String path, String branch})>{};
    for (final entry in liveEntries) {
      if (entry.path.isEmpty) {
        continue;
      }
      entries[_canonicalPath(entry.path)] = (
        path: entry.path,
        branch: entry.branch,
      );
    }
    return entries;
  }
}
