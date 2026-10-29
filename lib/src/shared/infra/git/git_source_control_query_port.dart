import 'package:alera/src/shared/infra/git/git_diff_models.dart';

/// The only Git reads needed to present the current workspace source-control
/// snapshot. A fake must not implement checkout, staging, push, or other
/// mutation capabilities.
abstract interface class GitSourceControlQueryPort {
  Future<GitStatusResult> status(String path);
  Future<GitRepositoryState> repositoryState(String path);
  Future<List<GitStashEntry>> listStashes(String path);
}
