import 'package:alera/src/features/workbench/domain/workspace.dart';

/// Resolves an existing managed checkout without changing Git or the UI.
/// Archived worktrees are included because archiving does not remove their
/// checkout; removed workspaces and other projects are not eligible.
Workspace? findExistingBranchWorktree({
  required Iterable<Workspace> workspaces,
  required Workspace currentWorkspace,
  required String branch,
}) {
  final name = branch.trim();
  if (name.isEmpty || name == 'HEAD') return null;
  for (final candidate in workspaces) {
    if (candidate.projectId == currentWorkspace.projectId &&
        candidate.id != currentWorkspace.id &&
        candidate.isActive &&
        candidate.branch == name) {
      return candidate;
    }
  }
  return null;
}
