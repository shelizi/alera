part of 'fake_git_backend.dart';

mixin _FakeGitBackendCommitOps {
  List<GitBackendCall> get calls;
  GitException? get revertCommitError;
  GitException? get resetToCommitError;
  String get revertCommitResult;

  Future<String> revertCommit({
    required String path,
    required String commitId,
    int? mainlineParent,
  }) async {
    calls.add(
      GitBackendCall('revertCommit', <String, Object?>{
        'path': path,
        'commitId': commitId,
        'mainlineParent': mainlineParent,
      }),
    );
    final error = revertCommitError;
    if (error != null) {
      throw error;
    }
    return revertCommitResult;
  }

  Future<void> resetToCommit({
    required String path,
    required String commitId,
    required GitResetMode mode,
  }) async {
    calls.add(
      GitBackendCall('resetToCommit', <String, Object?>{
        'path': path,
        'commitId': commitId,
        'mode': mode,
      }),
    );
    final error = resetToCommitError;
    if (error != null) {
      throw error;
    }
  }
}
