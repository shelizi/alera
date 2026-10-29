import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_working_tree_diff_query_port.dart';

/// Preview budget parity with the existing Dart UI: String.length counts
/// UTF-16 code units plus one logical separator per diff line.
/// This is not a UTF-8 allocation estimate.
int gitDiffPreviewBytes(GitDiffResult result) {
  var total = 0;
  for (final file in result.files) {
    for (final line in file.lines) {
      total += line.text.length + 1;
    }
  }
  return total;
}

class const WorkspaceGitDiffFirstPage({
  required final GitDiffResult result,
  required final int? nextIndex,
});

/// A narrowly injected source-control query owner for status-scoped diff
/// pagination. The UI owns rendering/cancellation; RustGitBackend owns IO.
final class WorkspaceGitDiffPageReader {
  const WorkspaceGitDiffPageReader(this._git);

  final GitWorkingTreeDiffQueryPort _git;

  Future<List<String>?> changedPaths(String path) async {
    final status = await _git.status(path);
    final hasNestedSubmoduleChanges = status.entries.any((entry) {
      final submodule = entry.submodule;
      return submodule != null &&
          (submodule.trackedChanges || submodule.untrackedChanges);
    });
    if (hasNestedSubmoduleChanges) {
      // Rust's combined diff also traverses nested submodule changes. Page
      // queries do not yet have an equivalent snapshot contract.
      return null;
    }
    final paths = <String>{};
    for (final entry in status.entries) {
      paths.add(entry.path);
    }
    return paths.toList(growable: false);
  }

  Future<WorkspaceGitDiffFirstPage> firstPage({
    required String path,
    required Future<List<String>?> pathsFuture,
    required GitDiffWhitespaceMode whitespaceMode,
    int pageSize = 1,
  }) async {
    final paths = await pathsFuture;
    if (paths == null) {
      return WorkspaceGitDiffFirstPage(
        result: await _git.diffAll(path: path, whitespaceMode: whitespaceMode),
        nextIndex: null,
      );
    }
    if (paths.isEmpty) {
      return const WorkspaceGitDiffFirstPage(
        result: GitDiffResult(files: []),
        nextIndex: null,
      );
    }
    final page = await _git.diffAllPage(
      path: path,
      filePaths: paths.take(pageSize).toList(growable: false),
      whitespaceMode: whitespaceMode,
    );
    return WorkspaceGitDiffFirstPage(
      result: GitDiffResult(files: page.files, truncated: page.truncated),
      nextIndex: paths.length > pageSize ? pageSize : null,
    );
  }

  Future<GitDiffPage?> nextPage({
    required String path,
    required List<String> filePaths,
    required GitDiffWhitespaceMode whitespaceMode,
  }) async {
    try {
      return await _git.diffAllPage(
        path: path,
        filePaths: filePaths,
        whitespaceMode: whitespaceMode,
      );
    } catch (_) {
      // A subsequent page is best-effort once the first page is visible;
      // retain the already-rendered diff when a file disappears mid-scan.
      return null;
    }
  }
}
