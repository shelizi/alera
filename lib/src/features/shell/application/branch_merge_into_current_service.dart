import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';

/// Merges a selected local branch into the branch checked out in this workspace.
/// No checkout or linked worktree is created; conflicts stay in this workspace.
final class BranchMergeIntoCurrentService {
  const BranchMergeIntoCurrentService(this._git);

  final GitBackend _git;

  Future<BranchMergeIntoCurrentResult> merge({
    required String path,
    required String sourceBranch,
    required String expectedCurrentBranch,
  }) async {
    if (expectedCurrentBranch.isEmpty || expectedCurrentBranch == 'HEAD') {
      throw StateError('A checked-out branch is required for merging.');
    }
    if (sourceBranch.isEmpty || sourceBranch == expectedCurrentBranch) {
      throw StateError('Choose a different source branch.');
    }
    // The workspace may have switched branches while the dialog was open.
    if (await _git.currentBranch(path) != expectedCurrentBranch) {
      throw StateError('The current branch changed. Reopen the branch menu.');
    }
    if (!await _git.branchExists(path, sourceBranch)) {
      throw StateError('The selected local branch no longer exists.');
    }
    // A direct merge must never overwrite pending work in the active checkout.
    final status = await _git.status(path);
    final state = await _git.repositoryState(path);
    if (status.entries.isNotEmpty || state.hasConflicts) {
      throw StateError(
        'Commit or stash the current workspace changes and resolve conflicts before merging.',
      );
    }
    if (await _git.currentBranch(path) != expectedCurrentBranch) {
      throw StateError('The current branch changed. Reopen the branch menu.');
    }

    try {
      final commitId = await _git.mergeRef(path: path, ref: sourceBranch);
      return BranchMergeIntoCurrentResult(mergeCommitId: commitId);
    } on GitConflictException {
      // Keep MERGE_HEAD and conflicted files in this workspace for resolution.
      return const BranchMergeIntoCurrentResult(conflicted: true);
    }
  }
}

class const BranchMergeIntoCurrentResult({
  final String? mergeCommitId,
  final bool conflicted = false,
});
