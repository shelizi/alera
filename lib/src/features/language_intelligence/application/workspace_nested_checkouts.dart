/// Lists other checkouts that live inside a workspace directory, such as git
/// worktrees created under `.worktrees/`. Language servers that discover
/// projects recursively would otherwise analyze each of them as part of the
/// workspace.
abstract interface class WorkspaceNestedCheckoutsPort {
  /// Absolute paths strictly inside [workspaceRoot]; never the root itself.
  Future<List<String>> nestedCheckouts(String workspaceRoot);
}
