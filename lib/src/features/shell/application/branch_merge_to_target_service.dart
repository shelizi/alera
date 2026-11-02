import 'package:alera/src/shared/infra/files/path_identity.dart';
import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';

/// The source checkout is never used for the merge. Only a known-idle,
/// registered linked worktree for the target branch may be reused.
class BranchMergeToTargetService {
  const BranchMergeToTargetService(this._git);

  final GitBackend _git;

  Future<BranchMergeToTargetResult> merge({
    required String sourcePath,
    required String sourceBranch,
    required String targetBranch,
    required Set<String> idleWorktreePaths,
    required Future<String> Function(String targetBranch) createWorktree,
  }) async {
    if (sourceBranch.isEmpty || sourceBranch == 'HEAD') {
      throw StateError('Cannot merge from a detached HEAD.');
    }
    if (targetBranch.isEmpty || targetBranch == sourceBranch) {
      throw StateError('Choose a different target branch.');
    }
    // The menu may have been open while another client switched branches.
    if (await _git.currentBranch(sourcePath) != sourceBranch) {
      throw StateError('The source branch changed. Reopen the branch menu.');
    }
    if (!await _git.branchExists(sourcePath, targetBranch)) {
      throw StateError('The target local branch no longer exists.');
    }

    final worktrees = await _git.listWorktrees(sourcePath);
    final checkedOut = worktrees.where((entry) => entry.branch == targetBranch);
    late final String targetPath;
    var created = false;
    if (checkedOut.isNotEmpty) {
      // An open Alera tab, the current checkout, or an unregistered external
      // worktree is not safely idle. Never silently steal it.
      if (checkedOut.length != 1 ||
          isSamePath(checkedOut.first.path, sourcePath) ||
          !idleWorktreePaths.any(
            (path) => isSamePath(path, checkedOut.first.path),
          )) {
        throw StateError(
          'The target branch is checked out in a worktree that is in use.',
        );
      }
      targetPath = checkedOut.first.path;
    } else {
      targetPath = await createWorktree(targetBranch);
      created = true;
    }

    if (isSamePath(sourcePath, targetPath) ||
        await _git.currentBranch(targetPath) != targetBranch) {
      throw StateError('The merge worktree is not on the target branch.');
    }
    if ((await _git.status(targetPath)).entries.isNotEmpty ||
        (await _git.repositoryState(targetPath)).hasConflicts) {
      throw StateError(
        'The target worktree has uncommitted changes or conflicts.',
      );
    }

    try {
      final commitId = await _git.mergeRef(path: targetPath, ref: sourceBranch);
      return BranchMergeToTargetResult(
        path: targetPath,
        createdWorktree: created,
        mergeCommitId: commitId,
      );
    } on GitConflictException {
      // libgit2 leaves MERGE_HEAD and conflict entries in the *target*
      // checkout. Preserve that worktree for Source Control resolution.
      return BranchMergeToTargetResult(
        path: targetPath,
        createdWorktree: created,
        conflicted: true,
      );
    }
  }
}

class const BranchMergeToTargetResult({
  required final String path,
  required final bool createdWorktree,
  final String? mergeCommitId,
  final bool conflicted = false,
});
