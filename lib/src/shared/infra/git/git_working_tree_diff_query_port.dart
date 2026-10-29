import 'package:alera/src/shared/infra/git/git_diff_models.dart';

/// Narrow read-only capability for status-scoped worktree diff previews.
/// Keeps branch mutation/checkout/commit APIs out of progressive readers.
abstract interface class GitWorkingTreeDiffQueryPort {
  Future<GitStatusResult> status(String path);

  Future<GitDiffResult> diff({
    required String path,
    required String filePath,
    required GitChangeArea area,
    GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
  });

  Future<GitDiffResult> diffAll({
    required String path,
    String? filePath,
    GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
  });

  Future<GitDiffPage> diffAllPage({
    required String path,
    required List<String> filePaths,
    GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
  });
}
