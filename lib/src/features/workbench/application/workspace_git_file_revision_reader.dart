import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_file_revision_query_port.dart';

/// Resolves file revisions only within the currently selected repository root.
/// The UI owns tab activation; this service owns the path/query boundary.
final class WorkspaceGitFileRevisionReader {
  const WorkspaceGitFileRevisionReader(this._git);

  final GitFileRevisionQueryPort _git;

  Future<GitFileRevision?> latestForScopedFile({
    required WorkspaceSourceControlScope scope,
    required String workspaceRelativePath,
    required String gitRef,
  }) async {
    // Normalize even at the workspace root, where the scope's generic
    // translation intentionally leaves its argument unchanged.
    final normalized = normalizeWorkspaceRelativePath(workspaceRelativePath);
    if (normalized == null) return null;
    final filePath = scope.toSourceRelativePath(normalized);
    if (filePath == null || filePath.isEmpty) return null;
    return _git.latestFileRevision(
      path: scope.path,
      filePath: filePath,
      gitRef: gitRef,
    );
  }
}
