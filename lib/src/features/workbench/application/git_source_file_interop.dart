// Shared Classic Source Control selection and guarded worktree file transfer.
import 'dart:io';

import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:path/path.dart' as p;

const int gitSourceClipboardLimit = 24;
const int gitSourceDragLimit = 120;
const int gitSourceExternalDropLimit = 8;
const int _gitSourceDirectoryLimit = 4096;

class GitSourceFileItem {
  const GitSourceFileItem({
    required this.id,
    required this.relativePath,
    required this.isDirectory,
  });

  final String id;
  final String relativePath;
  final bool isDirectory;
}

String gitSourceFolderId(GitChangeGroup group, String relativePath) =>
    'folder:${group.unified ? 'unified' : group.area.key}:$relativePath';

List<GitSourceFileItem> visibleGitSourceFileItems({
  required List<GitChangeGroup> groups,
  required GitDiffViewMode viewMode,
  required Set<String> collapsedSections,
  required Set<String> collapsedTreeNodes,
}) {
  final items = <GitSourceFileItem>[];
  for (final group in groups) {
    final sectionId = group.unified
        ? 'section:unified'
        : 'section:${group.area.key}';
    if (collapsedSections.contains(sectionId)) continue;
    if (viewMode == GitDiffViewMode.flat) {
      for (final entry in group.entries) {
        items.add(
          GitSourceFileItem(
            id: entry.id,
            relativePath: entry.path,
            isDirectory: entry.submodule != null,
          ),
        );
      }
      continue;
    }
    int? collapsedDepth;
    for (final row in group.treeRows) {
      if (collapsedDepth case final depth?) {
        if (row.depth > depth) continue;
        collapsedDepth = null;
      }
      if (row.entry case final entry?) {
        items.add(
          GitSourceFileItem(
            id: entry.id,
            relativePath: entry.path,
            isDirectory: entry.submodule != null,
          ),
        );
      } else if (row.kind == GitChangeTreeRowKind.directory) {
        final id = gitSourceFolderId(group, row.path);
        items.add(
          GitSourceFileItem(id: id, relativePath: row.path, isDirectory: true),
        );
        if (collapsedTreeNodes.contains(id)) collapsedDepth = row.depth;
      }
    }
  }
  return items;
}

/// A right-click on an unselected row operates on that row only. A selected
/// row uses all currently visible selected rows; staged and unstaged instances
/// of a path are deduplicated, and parent folders subsume descendants.
List<String> gitSourceSelectedPaths({
  required List<GitSourceFileItem> visible,
  required Set<String> selectedIds,
  String? clickedId,
}) {
  final clicked = clickedId == null
      ? null
      : [
          for (final item in visible)
            if (item.id == clickedId) item,
        ].firstOrNull;
  if (clickedId != null && clicked == null) return const <String>[];
  final selected = clicked != null && !selectedIds.contains(clicked.id)
      ? <GitSourceFileItem>[clicked]
      : <GitSourceFileItem>[
          for (final item in visible)
            if (selectedIds.contains(item.id)) item,
        ];
  final relative = selected.map((item) => item.relativePath).toSet().toList()
    ..sort();
  final topLevel = <String>[];
  for (final path in relative) {
    if (!topLevel.any(
      (parent) => path == parent || path.startsWith('$parent/'),
    )) {
      topLevel.add(path);
    }
  }
  return topLevel;
}

/// Fail closed at the OS-file boundary, not just when building a Git row.
/// Native file drag/copy must not export a directory containing Git control
/// metadata or symlink/junction descendants.
class GitSourceFileGuard {
  const GitSourceFileGuard();

  Future<List<String>> checkedSourcePaths({
    required String workspacePath,
    required List<String> relativePaths,
    required int limit,
  }) async {
    if (relativePaths.isEmpty || relativePaths.length > limit) {
      throw const FileSystemException('Invalid Git file selection size');
    }
    final root = await Directory(workspacePath).resolveSymbolicLinks();
    final checked = <String>[];
    for (final relative in relativePaths) {
      final path = await _resolveChild(root, relative, mustBeDirectory: false);
      if (await FileSystemEntity.type(path, followLinks: false) ==
          FileSystemEntityType.directory) {
        var count = 0;
        await for (final entry in Directory(
          path,
        ).list(recursive: true, followLinks: false)) {
          count++;
          if (count > _gitSourceDirectoryLimit) {
            throw const FileSystemException(
              'Git folder exceeds transfer size limit',
            );
          }
          final nested = p.relative(entry.path, from: root);
          await _resolveChild(root, nested, mustBeDirectory: false);
        }
      }
      checked.add(path);
    }
    return checked;
  }

  Future<String> checkedDirectory({
    required String workspacePath,
    required String relativeDirectory,
  }) async {
    final root = await Directory(workspacePath).resolveSymbolicLinks();
    if (relativeDirectory.isEmpty) return root;
    return _resolveChild(root, relativeDirectory, mustBeDirectory: true);
  }

  Future<String> _resolveChild(
    String root,
    String relative, {
    required bool mustBeDirectory,
  }) async {
    if (relative.trim().isEmpty || p.isAbsolute(relative)) {
      throw FileSystemException('Not a worktree child', relative);
    }
    final rawComponents = p.split(relative.replaceAll(r'\', p.separator));
    if (rawComponents.contains('..')) {
      throw FileSystemException('Parent traversal is not allowed', relative);
    }
    final normalized = p.normalize(relative.replaceAll(r'\', p.separator));
    final components = p.split(normalized);
    if (normalized == '.' ||
        components.any(
          (component) =>
              component == '..' ||
              component == '.' ||
              component.toLowerCase() == '.git' ||
              component.toLowerCase() == '.worktrees',
        )) {
      throw FileSystemException('Protected worktree path', relative);
    }
    var current = root;
    for (final segment in components) {
      current = p.join(current, segment);
      final type = await FileSystemEntity.type(current, followLinks: false);
      if (type == FileSystemEntityType.link ||
          type == FileSystemEntityType.notFound ||
          (type != FileSystemEntityType.directory &&
              type != FileSystemEntityType.file)) {
        throw FileSystemException('Unsafe or missing worktree path', current);
      }
    }
    if (!p.isWithin(root, current)) {
      throw FileSystemException('Path outside worktree', current);
    }
    if (mustBeDirectory &&
        await FileSystemEntity.type(current, followLinks: false) !=
            FileSystemEntityType.directory) {
      throw FileSystemException('Destination is not a folder', current);
    }
    return current;
  }

  /// Predictable basename collisions and directory cycles must refuse the
  /// complete batch before the first backend copy/move.
  Future<void> preflightDestination({
    required List<String> absoluteSources,
    required String destinationFolder,
  }) async {
    final names = <String>{};
    for (final source in absoluteSources) {
      if (p.equals(source, destinationFolder) ||
          p.isWithin(source, destinationFolder)) {
        throw FileSystemException(
          'Cannot copy or move a folder into itself',
          source,
        );
      }
      final name = p.basename(source).toLowerCase();
      if (!names.add(name)) {
        throw FileSystemException(
          'Two selected items share a destination name',
          name,
        );
      }
      final target = p.join(destinationFolder, p.basename(source));
      if (await FileSystemEntity.type(target, followLinks: false) !=
          FileSystemEntityType.notFound) {
        throw FileSystemException('Destination already exists', target);
      }
    }
  }
}
