import 'package:alera/src/shared/infra/git/git_diff_models.dart';

/// Six scoped source-control index/worktree writes. The production Git backend
/// remains a single Rust-backed implementation, but consumers can fake only
/// the mutation capabilities they own.
abstract interface class GitIndexMutationPort {
  Future<void> stage({required String path, String? filePath});
  Future<void> stageArea({
    required String path,
    required GitChangeArea area,
    String? filePath,
  });
  Future<void> unstage({required String path, String? filePath});
  Future<void> unstageArea({
    required String path,
    required GitChangeArea area,
    String? filePath,
  });
  Future<void> discard({required String path, String? filePath});
  Future<void> discardArea({
    required String path,
    required GitChangeArea area,
    String? filePath,
  });
}
