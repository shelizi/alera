import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';

/// Resolves a read-only comparison between the checked-out branch and a
/// selected local branch. The returned commit OIDs pin the existing Diff tab.
final class BranchCompareCurrentService {
  const BranchCompareCurrentService(this._backend);

  final GitBackend _backend;

  Future<BranchCompareCurrentSnapshot> prepare({
    required String path,
    required String compareBranch,
  }) async {
    final current = await _backend.currentBranch(path);
    if (current.isEmpty || current == 'HEAD') {
      throw const DetachedHeadException(
        'A checked-out branch is required to compare branches.',
      );
    }
    if (compareBranch.isEmpty || current == compareBranch) {
      throw StateError('Choose a different branch to compare.');
    }
    if (!await _backend.branchExists(path, compareBranch)) {
      throw BranchNotFoundException(compareBranch);
    }
    final compare = await _backend.compareRange(
      path: path,
      baseRef: current,
      headRef: compareBranch,
    );
    final mergeBase = compare.summary.parentOid;
    if (compare.summary.status != GitCommitCompareStatus.ready ||
        mergeBase == null) {
      throw GitInternalException(
        compare.summary.errorMessage ??
            'Could not find a common Git ancestor for these branches.',
      );
    }
    if (await _backend.currentBranch(path) != current) {
      throw StateError('The current branch changed. Reopen the branch menu.');
    }
    return BranchCompareCurrentSnapshot(
      baseRef: current,
      headRef: compareBranch,
      headOid: compare.summary.commitOid,
      mergeBaseOid: mergeBase,
    );
  }
}

class const BranchCompareCurrentSnapshot({
  required this.baseRef,
  required this.headRef,
  required this.headOid,
  required this.mergeBaseOid,
}) {
  final String baseRef;
  final String headRef;
  final String headOid;
  final String mergeBaseOid;
}
