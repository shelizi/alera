/// Lists the file names that decide which languages a workspace uses.
abstract interface class WorkspaceMarkerFilesPort {
  /// Names of the files directly in [workspaceRoot] and in its immediate
  /// subdirectories. This is the depth at which language servers such as
  /// rust-analyzer look for projects themselves.
  Future<Set<String>> markerCandidates(String workspaceRoot);
}
