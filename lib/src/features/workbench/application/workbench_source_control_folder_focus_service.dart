import 'dart:io';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:path/path.dart' as p;

abstract interface class WorkbenchGitRepositoryProbe {
  Future<bool> isGitRepository(String path);
}

typedef WorkbenchDirectGitEntryProbe = bool Function(String path);

final class WorkbenchSourceControlFolderFocusService {
  WorkbenchSourceControlFolderFocusService({
    required WorkbenchGitRepositoryProbe gitRepositoryProbe,
    WorkbenchDirectGitEntryProbe? hasDirectGitEntry,
  }) : _gitRepositoryProbe = gitRepositoryProbe,
       _hasDirectGitEntry = hasDirectGitEntry ?? _defaultHasDirectGitEntry;

  final WorkbenchGitRepositoryProbe _gitRepositoryProbe;
  final WorkbenchDirectGitEntryProbe _hasDirectGitEntry;

  Future<String?> resolve({
    required Project project,
    required Workspace workspace,
    required String relativePath,
    required bool Function() isSelectionActive,
  }) async {
    if (!project.isFolder || !isSelectionActive()) {
      return null;
    }
    final normalized = normalizeSourceControlRootRelativePath(relativePath);
    if (normalized == null) {
      return null;
    }
    final path = sourceControlRootAbsolutePath(
      workspacePath: workspace.path,
      relativeRoot: normalized,
    );
    if (!_hasDirectGitEntry(path)) {
      return null;
    }
    if (!await _gitRepositoryProbe.isGitRepository(path)) {
      return null;
    }
    if (!isSelectionActive()) {
      return null;
    }
    return normalized;
  }

  static bool _defaultHasDirectGitEntry(String path) {
    final gitEntryPath = p.join(path, '.git');
    return Directory(gitEntryPath).existsSync() ||
        File(gitEntryPath).existsSync();
  }
}
