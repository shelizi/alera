/// Read-only repository identity check for workspace/folder reveal.
///
/// A probe never needs branch, status, diff, or mutation operations.
abstract interface class GitRepositoryProbePort {
  Future<bool> isGitRepository(String path);
}
