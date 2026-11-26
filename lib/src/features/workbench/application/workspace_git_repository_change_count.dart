import 'package:alera/src/shared/infra/git/git_diff_models.dart';

/// The repository overview displays only the number of distinct changed paths.
/// Use the authoritative lightweight path query where available. A null result
/// indicates dirty nested submodules: preserve the original full-status count
/// rather than interpreting it as zero or counting inner diff files.
Future<int> loadWorkspaceGitRepositoryChangeCount({
  required String path,
  required Future<List<String>?> Function(String path) loadChangedPaths,
  required Future<GitStatusResult> Function(String path) loadStatus,
}) async {
  final changed = await loadChangedPaths(path);
  if (changed != null) {
    return changed.length;
  }
  final status = await loadStatus(path);
  return status.entries.map((entry) => entry.path).toSet().length;
}
