import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_index_mutation_port.dart';

/// Preserves rename old-path ordering and submodule safety outside the UI.
final class WorkspaceSourceControlIndexMutations {
  const WorkspaceSourceControlIndexMutations(this._git);
  final GitIndexMutationPort _git;

  Future<void> stage(String path, String? filePath) =>
      _git.stage(path: path, filePath: filePath);
  Future<void> unstage(String path, String? filePath) =>
      _git.unstage(path: path, filePath: filePath);
  Future<void> discard(String path, String? filePath) =>
      _git.discard(path: path, filePath: filePath);
  Future<void> stageArea(String path, GitChangeArea area, {String? filePath}) =>
      _git.stageArea(path: path, area: area, filePath: filePath);
  Future<void> unstageArea(
    String path,
    GitChangeArea area, {
    String? filePath,
  }) => _git.unstageArea(path: path, area: area, filePath: filePath);
  Future<void> discardArea(
    String path,
    GitChangeArea area, {
    String? filePath,
  }) => _git.discardArea(path: path, area: area, filePath: filePath);

  Future<void> stageEntry(String path, GitChangeEntry entry) async {
    if (!entry.canStageFromParent) return;
    for (final filePath in _actionPaths(entry)) {
      await stage(path, filePath);
    }
  }

  Future<void> unstageEntry(String path, GitChangeEntry entry) async {
    if (!entry.canUnstageFromParent) return;
    for (final filePath in _actionPaths(entry)) {
      await unstage(path, filePath);
    }
  }

  Future<void> discardEntry(String path, GitChangeEntry entry) async {
    if (!entry.canDiscardFromParent) return;
    for (final filePath in _actionPaths(entry)) {
      await discard(path, filePath);
    }
  }

  static List<String> _actionPaths(GitChangeEntry entry) {
    final paths = <String>[];
    final oldPath = entry.oldPath;
    if (entry.status == GitChangeStatus.renamed &&
        oldPath != null &&
        oldPath != entry.path) {
      paths.add(oldPath);
    }
    paths.add(entry.path);
    return paths;
  }
}
