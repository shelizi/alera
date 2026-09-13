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

part 'workspace_service_linked.dart';
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

    // Keep user-renamed names, but re-derive the name when the stored one was
    // auto-produced (the previous default or the project-name fallback used
    // while the branch was unresolvable), so a stale "HEAD" era name does not
    // stick after the real branch is known.
    final existingName = mainWorkspace?.name;
    final previousDefault = mainWorkspace == null
        ? null
        : _defaultMainWorkspaceName(mainWorkspace.branch, project.name);
    final preserveName =
        (mainWorkspace?.isMain ?? false) &&
        existingName != previousDefault &&
        existingName != project.name;
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
              name: preserveName ? existingName! : defaultName,
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
}
