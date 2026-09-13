part of 'fake_git_backend.dart';

mixin _FakeGitBackendCommitOps {
  List<GitBackendCall> get calls;
  GitException? get checkoutCommitError;
  GitException? get revertCommitError;
  GitException? get resetToCommitError;
  String get revertCommitResult;
  GitException? get createBranchAtCommitError;
  GitException? get cherryPickCommitError;
  String get cherryPickCommitResult;
  GitException? get dropCommitError;
  GitException? get mergeRefError;
  String? get mergeRefResult;
  GitException? get rebaseOntoError;
  GitException? get createTagError;
  GitException? get deleteTagError;
  GitException? get pushTagError;
  GitException? get checkoutRemoteBranchError;
  String get checkoutRemoteBranchResult;
  GitException? get deleteRemoteBranchError;
  GitException? get renameBranchError;
  GitException? get createArchiveError;
  GitException? get compareRangeError;
  GitCommitCompareResult get compareRangeResult;

  Future<void> checkoutCommit({
    required String path,
    required String commitId,
  }) async {
    calls.add(
      GitBackendCall('checkoutCommit', <String, Object?>{
        'path': path,
        'commitId': commitId,
      }),
    );
    final error = checkoutCommitError;
    if (error != null) {
      throw error;
    }
  }

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

  Future<void> createBranchAtCommit({
    required String path,
    required String commitId,
    required String branch,
    bool checkout = false,
  }) async {
    calls.add(
      GitBackendCall('createBranchAtCommit', <String, Object?>{
        'path': path,
        'commitId': commitId,
        'branch': branch,
        'checkout': checkout,
      }),
    );
    final error = createBranchAtCommitError;
    if (error != null) {
      throw error;
    }
  }

  Future<String> cherryPickCommit({
    required String path,
    required String commitId,
    int? mainlineParent,
  }) async {
    calls.add(
      GitBackendCall('cherryPickCommit', <String, Object?>{
        'path': path,
        'commitId': commitId,
        'mainlineParent': mainlineParent,
      }),
    );
    final error = cherryPickCommitError;
    if (error != null) {
      throw error;
    }
    return cherryPickCommitResult;
  }

  Future<void> dropCommit({
    required String path,
    required String commitId,
  }) async {
    calls.add(
      GitBackendCall('dropCommit', <String, Object?>{
        'path': path,
        'commitId': commitId,
      }),
    );
    final error = dropCommitError;
    if (error != null) {
      throw error;
    }
  }

  Future<String?> mergeRef({
    required String path,
    required String ref,
  }) async {
    calls.add(
      GitBackendCall('mergeRef', <String, Object?>{'path': path, 'ref': ref}),
    );
    final error = mergeRefError;
    if (error != null) {
      throw error;
    }
    return mergeRefResult;
  }

  Future<void> rebaseOnto({
    required String path,
    required String ontoRef,
  }) async {
    calls.add(
      GitBackendCall('rebaseOnto', <String, Object?>{
        'path': path,
        'ontoRef': ontoRef,
      }),
    );
    final error = rebaseOntoError;
    if (error != null) {
      throw error;
    }
  }

  Future<void> createTag({
    required String path,
    required String commitId,
    required String name,
    String? message,
  }) async {
    calls.add(
      GitBackendCall('createTag', <String, Object?>{
        'path': path,
        'commitId': commitId,
        'name': name,
        'message': message,
      }),
    );
    final error = createTagError;
    if (error != null) {
      throw error;
    }
  }

  Future<void> deleteTag({required String path, required String name}) async {
    calls.add(
      GitBackendCall('deleteTag', <String, Object?>{'path': path, 'name': name}),
    );
    final error = deleteTagError;
    if (error != null) {
      throw error;
    }
  }

  Future<void> pushTag({
    required String path,
    required String name,
    String? remote,
  }) async {
    calls.add(
      GitBackendCall('pushTag', <String, Object?>{
        'path': path,
        'name': name,
        'remote': remote,
      }),
    );
    final error = pushTagError;
    if (error != null) {
      throw error;
    }
  }

  Future<String> checkoutRemoteBranch({
    required String path,
    required String remoteBranch,
  }) async {
    calls.add(
      GitBackendCall('checkoutRemoteBranch', <String, Object?>{
        'path': path,
        'remoteBranch': remoteBranch,
      }),
    );
    final error = checkoutRemoteBranchError;
    if (error != null) {
      throw error;
    }
    return checkoutRemoteBranchResult;
  }

  Future<void> deleteRemoteBranch({
    required String path,
    required String remote,
    required String branch,
  }) async {
    calls.add(
      GitBackendCall('deleteRemoteBranch', <String, Object?>{
        'path': path,
        'remote': remote,
        'branch': branch,
      }),
    );
    final error = deleteRemoteBranchError;
    if (error != null) {
      throw error;
    }
  }

  Future<void> renameBranch({
    required String path,
    required String oldName,
    required String newName,
  }) async {
    calls.add(
      GitBackendCall('renameBranch', <String, Object?>{
        'path': path,
        'oldName': oldName,
        'newName': newName,
      }),
    );
    final error = renameBranchError;
    if (error != null) {
      throw error;
    }
  }

  Future<void> createArchive({
    required String path,
    required String ref,
    required String outputPath,
    GitArchiveFormat format = GitArchiveFormat.zip,
  }) async {
    calls.add(
      GitBackendCall('createArchive', <String, Object?>{
        'path': path,
        'ref': ref,
        'outputPath': outputPath,
        'format': format,
      }),
    );
    final error = createArchiveError;
    if (error != null) {
      throw error;
    }
  }

  Future<GitCommitCompareResult> compareRange({
    required String path,
    required String baseRef,
    required String headRef,
  }) async {
    calls.add(
      GitBackendCall('compareRange', <String, Object?>{
        'path': path,
        'baseRef': baseRef,
        'headRef': headRef,
      }),
    );
    final error = compareRangeError;
    if (error != null) {
      throw error;
    }
    return compareRangeResult;
  }
}
