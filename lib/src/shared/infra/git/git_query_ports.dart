import 'package:alera/src/shared/infra/git/git_diff_models.dart';

/// Narrow read capability for paged Git history. Implementations may share one
/// production backend without forcing application consumers to fake mutations.
abstract interface class GitHistoryQueryPort {
  Future<GitHistoryResult> history(
    String path, {
    int? limit,
    String? baseRef,
    bool includeAllRefs = false,
    List<String>? includedRefs,
    int? offset,
    String? filePath,
  });
}

/// Read-only capability for comparing a commit with its first parent.
abstract interface class GitCommitCompareQueryPort {
  Future<GitCommitCompareResult> commitCompare({
    required String path,
    required String commitId,
  });
}
