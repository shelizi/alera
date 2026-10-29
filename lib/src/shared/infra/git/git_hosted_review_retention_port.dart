/// Lifetime management of immutable objects retained for hosted review tabs.
///
/// This capability deliberately excludes fetch, branch switching and push.
abstract interface class GitHostedReviewRetentionPort {
  Future<void> persistHostedReviewRange({
    required String path,
    required String retentionId,
  });

  Future<void> releaseHostedReviewRange({
    required String path,
    required String retentionId,
  });
}
