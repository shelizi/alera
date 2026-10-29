import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_source_control_query_port.dart';

class const WorkspaceSourceControlSnapshot({
  required this.status,
  required this.repositoryState,
  required this.stashes,
}) {
  final GitStatusResult status;
  final GitRepositoryState repositoryState;
  final List<GitStashEntry> stashes;
}

/// Executes all three independent read-only Git queries concurrently.
/// The source-control controller still owns state reconciliation and watcher
/// generations; no second Git owner or async cache is introduced here.
final class WorkspaceSourceControlSnapshotLoader {
  const WorkspaceSourceControlSnapshotLoader(this._reader);
  final GitSourceControlQueryPort _reader;

  Future<WorkspaceSourceControlSnapshot> load(String workspacePath) async {
    final results = await Future.wait<Object>(<Future<Object>>[
      _reader.status(workspacePath),
      _reader.repositoryState(workspacePath),
      _reader.listStashes(workspacePath),
    ]);
    return WorkspaceSourceControlSnapshot(
      status: results[0] as GitStatusResult,
      repositoryState: results[1] as GitRepositoryState,
      stashes: results[2] as List<GitStashEntry>,
    );
  }
}
