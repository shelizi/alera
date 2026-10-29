import 'dart:typed_data';

import 'package:alera/src/features/workbench/application/workspace_git_branch_compare_cache.dart';
import 'package:alera/src/shared/infra/git/git_diff_blob_query_port.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';

/// Owns side-specific Git blob access and immutable branch compare caching;
/// the diff widget retains decoding, rendering and cancellation ownership.
final class WorkspaceGitDiffBlobReader {
  const WorkspaceGitDiffBlobReader(this._git);

  final GitDiffBlobQueryPort _git;

  Future<Uint8List?> loadSide({
    required String path,
    required GitDiffFile file,
    required bool oldSide,
    required bool isFileRevisionDiff,
    required bool isCommitBackedDiff,
    String? revisionOid,
    String? parentOid,
    WorkspaceGitBranchCompareCache? branchCompareCache,
  }) async {
    try {
      if (isFileRevisionDiff) {
        if (revisionOid == null) return null;
        return await _git.fileRevisionBlobBytes(
          path: path,
          filePath: file.path,
          revisionOid: revisionOid,
          oldSide: oldSide,
        );
      }
      Future<Uint8List?> load() => _git.diffBlobBytes(
        path: path,
        filePath: file.path,
        oldPath: file.oldPath,
        area: isCommitBackedDiff ? null : file.area,
        commitOid: isCommitBackedDiff ? revisionOid : null,
        parentOid: isCommitBackedDiff ? parentOid : null,
        oldSide: oldSide,
      );
      if (branchCompareCache == null || revisionOid == null) {
        return await load();
      }
      return await branchCompareCache.blob(
        key: (path, revisionOid, parentOid, file.path, file.oldPath),
        oldSide: oldSide,
        load: load,
      );
    } catch (_) {
      // Native blobs can disappear after status refresh; keep the last
      // rendered preview usable, as the original UI did.
      return null;
    }
  }
}
