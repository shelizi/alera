part of 'rust_git_backend.dart';

mixin _RustGitBackendParityOps {
  Future<T> _guard<T>(Future<T> Function() body);

  Future<void> createBranchAtCommit({
    required String path,
    required String commitId,
    required String branch,
    bool checkout = false,
  }) => _guard(
    () => rust_branch.createBranchAtCommit(
      path: path,
      commitId: commitId,
      branch: branch,
      checkout: checkout,
    ),
  );

  Future<String> cherryPickCommit({
    required String path,
    required String commitId,
    int? mainlineParent,
  }) => _guard(
    () => rust_commit_ops.gitCherryPickCommit(
      path: path,
      commitId: commitId,
      mainlineParent: mainlineParent,
    ),
  );

  Future<void> dropCommit({required String path, required String commitId}) =>
      _guard(
        () => rust_commit_ops.gitDropCommit(path: path, commitId: commitId),
      );

  Future<String?> mergeRef({required String path, required String ref}) =>
      _guard(() => rust_merge_ops.mergeRef(path: path, reference: ref));

  Future<void> rebaseOnto({required String path, required String ontoRef}) =>
      _guard(() => rust_commit_ops.gitRebaseOnto(path: path, ontoRef: ontoRef));

  Future<void> createTag({
    required String path,
    required String commitId,
    required String name,
    String? message,
  }) => _guard(
    () => rust_tag_ops.createTag(
      path: path,
      commitId: commitId,
      name: name,
      message: message,
    ),
  );

  Future<void> deleteTag({required String path, required String name}) =>
      _guard(() => rust_tag_ops.deleteTag(path: path, name: name));

  Future<void> pushTag({
    required String path,
    required String name,
    String? remote,
  }) => _guard(
    () => rust_remote_ops.pushTag(path: path, name: name, remote: remote),
  );

  Future<String> checkoutRemoteBranch({
    required String path,
    required String remoteBranch,
  }) => _guard(
    () => rust_branch.checkoutRemoteBranch(
      path: path,
      remoteBranch: remoteBranch,
    ),
  );

  Future<void> deleteRemoteBranch({
    required String path,
    required String remote,
    required String branch,
  }) => _guard(
    () => rust_remote_ops.deleteRemoteBranch(
      path: path,
      remote: remote,
      branch: branch,
    ),
  );

  Future<void> renameBranch({
    required String path,
    required String oldName,
    required String newName,
  }) => _guard(
    () => rust_branch.renameBranch(
      path: path,
      oldName: oldName,
      newName: newName,
    ),
  );

  Future<void> createArchive({
    required String path,
    required String ref,
    required String outputPath,
    GitArchiveFormat format = GitArchiveFormat.zip,
  }) => _guard(
    () => rust_archive_ops.createArchive(
      path: path,
      reference: ref,
      outputPath: outputPath,
      format: _toRustArchiveFormat(format),
    ),
  );

}
