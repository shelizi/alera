import 'package:alera/src/shared/infra/git/git_diff_models.dart';

/// Read-only capability for resolving the version of a file on an exact Git
/// ref. Does not expose the broad backend's branch or index mutations.
abstract interface class GitFileRevisionQueryPort {
  Future<GitFileRevision?> latestFileRevision({
    required String path,
    required String filePath,
    required String gitRef,
  });
}
