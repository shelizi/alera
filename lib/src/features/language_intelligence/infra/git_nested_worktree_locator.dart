import '../../../shared/infra/files/path_identity.dart';
import '../../../shared/infra/git/git_worktree_entry.dart';
import '../application/workspace_nested_checkouts.dart';

typedef GitWorktreeLister = Future<List<GitWorktreeEntry>> Function(
  String repoPath,
);

/// Finds nested checkouts from the repository's own worktree records rather
/// than a fixed list of directory names, so a worktree placed anywhere under
/// the workspace is covered and an unrelated folder with the same name is not.
final class GitNestedWorktreeLocator implements WorkspaceNestedCheckoutsPort {
  const GitNestedWorktreeLocator(this._listWorktrees);

  final GitWorktreeLister _listWorktrees;

  @override
  Future<List<String>> nestedCheckouts(String workspaceRoot) async {
    final entries = await _listWorktrees(workspaceRoot);
    return <String>[
      for (final entry in entries)
        if (!isSamePath(workspaceRoot, entry.path) &&
            isPathWithinOrSame(workspaceRoot, entry.path))
          comparablePath(entry.path),
    ];
  }
}
