import 'dart:io';

import 'package:alera/src/features/projects/application/project_service.dart';
import 'package:alera/src/features/projects/application/project_config_service.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_repository.dart';
import 'package:alera/src/features/workbench/application/worktree_setup_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';
import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';
import 'package:alera/src/shared/infra/git/git_worktree_entry.dart';
import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

part 'workspace_service_reconciliation.dart';
part 'workspace_service_removal.dart';

class WorkspaceException(final String message, {final String? stderr})
    implements Exception {
  @override
  String toString() {
    final stderr = this.stderr?.trim();
    if (stderr == null || stderr.isEmpty) {
      return message;
    }
    return '$message: $stderr';
  }
}

/// Resolves the on-disk root for Alera-managed workspaces. Linked workspaces
/// are implemented as Git worktrees under this root.
class WorkspaceRoot._(
  final String? override,
  final Map<String, String> _environment,
) {
  factory({String? override, Map<String, String>? environment}) {
    return WorkspaceRoot._(override, environment ?? Platform.environment);
  }

  String resolve() {
    final explicit = override;
    if (explicit != null) {
      return explicit;
    }
    final home = _environment['HOME'] ?? _environment['USERPROFILE'];
    if (home == null || home.isEmpty) {
      throw WorkspaceException('Cannot locate user home directory');
    }
    return p.join(home, '.alera', 'workspaces');
  }
}

abstract interface class ManagedWorkspaceRuntime {
  Future<WorkspaceCreationResult> createLinkedWorkspace({
    required Project project,
    required String sourceBranch,
    required String newBranchName,
    required bool reuseExistingBranch,
    String? name,
  });

  Future<Workspace> switchWorkspaceBranch({
    required Workspace workspace,
    required String branch,
  });

  Future<void> removeWorkspace({
    required Workspace workspace,
    bool? deleteBranch,
    String? activeWorkspaceId,
  });
}

class WorkspaceService._(
  final WorkbenchRepository _repository,
  final ProjectService _projectService,
  final GitBackend _gitBackend,
  final WorkspaceRoot _workspaceRoot,
  final ProjectConfigReader _projectConfigReader,
  final WorktreeSetupRunner _worktreeSetupRunner,
  final ManagedWorkspaceRuntime? _managedRuntime,
  final Uuid _uuid,
  final DateTime Function() _now,
) {
  factory({
    required WorkbenchRepository repository,
    required ProjectService projectService,
    required GitBackend gitBackend,
    WorkspaceRoot? workspaceRoot,
    ProjectConfigReader? projectConfigReader,
    WorktreeSetupRunner? worktreeSetupRunner,
    ManagedWorkspaceRuntime? managedRuntime,
    Uuid? uuid,
    DateTime Function()? now,
  }) {
    return WorkspaceService._(
      repository,
      projectService,
      gitBackend,
      workspaceRoot ?? WorkspaceRoot(),
      projectConfigReader ?? const NoopProjectConfigReader(),
      worktreeSetupRunner ?? const NoopWorktreeSetupRunner(),
      managedRuntime,
      uuid ?? const Uuid(),
      now ?? _defaultNow,
    );
  }

  static DateTime _defaultNow() => DateTime.now().toUtc();

  Future<List<String>> listSourceBranches(Project project) {
    if (!project.supportsLinkedWorkspaces) {
      return Future<List<String>>.value(const <String>[]);
    }
    return _projectService.listGitBranches(project.repoPath);
  }

  Future<Workspace> ensureMainWorkspace(Project project) async {
    final existing = await _repository.listWorkspaces(project.id);
    final branch = project.isGitRepository
        ? await _currentBranch(project.repoPath)
        : null;
    final defaultName = _defaultMainWorkspaceName(branch, project.name);
    final now = _now();
    final projectPath = _canonicalPath(project.repoPath);

    Workspace? mainWorkspace;
    for (final workspace in existing) {
      if (workspace.isMain && _canonicalPath(workspace.path) == projectPath) {
        mainWorkspace = workspace;
        break;
      }
    }
    if (mainWorkspace == null) {
      for (final workspace in existing) {
        if (_canonicalPath(workspace.path) == projectPath) {
          mainWorkspace = workspace;
          break;
        }
      }
    }
    if (mainWorkspace == null) {
      for (final workspace in existing) {
        if (workspace.isMain) {
          mainWorkspace = workspace;
          break;
        }
      }
    }

    final preserveName = mainWorkspace?.isMain ?? false;
    final next =
        (mainWorkspace ??
                Workspace(
                  id: _uuid.v4(),
                  projectId: project.id,
                  name: defaultName,
                  branch: branch,
                  path: project.repoPath,
                  createdAt: now,
                  updatedAt: now,
                  kind: .main,
                  status: .active,
                ))
            .copyWith(
              name: preserveName ? mainWorkspace!.name : defaultName,
              branch: branch,
              path: project.repoPath,
              updatedAt: now,
              kind: .main,
              status: .active,
              sourceBranch: null,
              reusesExistingBranch: false,
            );
    await _repository.upsertWorkspace(next);

    for (final workspace in existing) {
      if (workspace.id == next.id) {
        continue;
      }
      if (_canonicalPath(workspace.path) == projectPath) {
        await _repository.removeWorkspace(workspace.id, cascadeTabs: true);
        continue;
      }
      if (workspace.isMain) {
        await _repository.upsertWorkspace(
          workspace.copyWith(
            kind: .linked,
            updatedAt: now,
            sourceBranch: null,
            reusesExistingBranch: true,
          ),
        );
      }
    }
    return next;
  }

  Future<Workspace> renameWorkspace({
    required String workspaceId,
    required String name,
  }) async {
    final trimmedName = name.trim();
    if (trimmedName.isEmpty) {
      throw WorkspaceException('Workspace name must not be empty');
    }
    final workspace = await _repository.findWorkspaceById(workspaceId);
    if (workspace == null) {
      throw WorkspaceException('Workspace not found: $workspaceId');
    }
    final next = workspace.copyWith(name: trimmedName, updatedAt: _now());
    await _repository.upsertWorkspace(next);
    return next;
  }

  Future<Workspace> switchWorkspaceBranch({
    required Project project,
    required Workspace workspace,
    required String branch,
  }) async {
    if (!project.supportsLinkedWorkspaces) {
      throw WorkspaceException(
        'Branch switching requires a Git repository project',
      );
    }
    if (workspace.projectId != project.id) {
      throw WorkspaceException(
        'Workspace does not belong to project ${project.id}',
      );
    }
    final targetBranch = branch.trim();
    if (targetBranch.isEmpty) {
      throw WorkspaceException('Branch name is required');
    }
    await _validateBranchName(targetBranch);
    if (workspace.branch == targetBranch) {
      return workspace;
    }
    await _ensureTargetBranchExists(project, targetBranch);
    final workspaces = await _repository.listWorkspaces(project.id);
    if (workspaces.any(
      (candidate) =>
          candidate.id != workspace.id &&
          candidate.isActive &&
          candidate.branch == targetBranch,
    )) {
      throw WorkspaceException(
        'A workspace for branch "$targetBranch" already exists',
      );
    }

    final managedRuntime = _managedRuntime;
    if (managedRuntime != null) {
      return managedRuntime.switchWorkspaceBranch(
        workspace: workspace,
        branch: targetBranch,
      );
    }

    final previousBranch = workspace.branch?.trim();
    try {
      await _gitBackend.checkoutBranch(
        path: workspace.path,
        branch: targetBranch,
      );
    } on GitException catch (error) {
      throw WorkspaceException('git checkout failed', stderr: error.context);
    }

    final next = workspace.copyWith(
      branch: targetBranch,
      sourceBranch: null,
      // Once a linked workspace moves away from the branch Alera created for
      // it, that checkout is now borrowing an existing branch. Never let later
      // cleanup delete that branch as though Alera owned it.
      reusesExistingBranch: workspace.isMain ? false : true,
      updatedAt: _now(),
    );
    try {
      return await _repository.upsertWorkspace(next);
    } catch (error) {
      if (previousBranch != null &&
          previousBranch.isNotEmpty &&
          previousBranch != 'HEAD' &&
          previousBranch != targetBranch) {
        try {
          await _gitBackend.checkoutBranch(
            path: workspace.path,
            branch: previousBranch,
          );
        } on GitException {
          // Keep the original persistence failure. The caller will refresh the
          // runtime snapshot and surface the inconsistent checkout if rollback
          // itself could not be completed.
        }
      }
      throw WorkspaceException(
        'Could not persist workspace branch switch',
        stderr: error.toString(),
      );
    }
  }

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

  Future<void> _validateBranchName(String branchName) async {
    final bool valid;
    try {
      valid = await _gitBackend.isValidBranchName(branchName);
    } on GitException catch (error) {
      throw WorkspaceException(
        'Invalid branch name "$branchName"',
        stderr: error.context,
      );
    }
    if (!valid) {
      throw WorkspaceException('Invalid branch name "$branchName"');
    }
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

  Future<void> _ensureTargetBranchExists(
    Project project,
    String branchName,
  ) async {
    final exists = await _gitBackend.branchExists(project.repoPath, branchName);
    if (!exists) {
      throw WorkspaceException('Branch "$branchName" does not exist');
    }
  }

  Future<String> _currentBranch(String repoPath) async {
    try {
      return await _gitBackend.currentBranch(repoPath);
    } on GitException {
      return 'HEAD';
    }
  }

  String _defaultMainWorkspaceName(String? branch, String projectName) {
    final trimmedBranch = branch?.trim();
    if (trimmedBranch == null ||
        trimmedBranch.isEmpty ||
        trimmedBranch == 'HEAD') {
      return projectName;
    }
    return trimmedBranch;
  }

  /// Resolves a workspace root to its real on-disk location so symlinked paths
  /// compare consistently. Falls back to canonical string normalization when
  /// the path no longer exists.
  String _canonicalPath(String path) {
    try {
      return p.canonicalize(Directory(path).resolveSymbolicLinksSync());
    } catch (_) {
      return p.canonicalize(path);
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
