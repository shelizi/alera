import 'dart:io';

import 'package:path/path.dart' as p;

import '../application/workspace_marker_files.dart';

/// Directories that hold dependencies or build output rather than the
/// workspace's own sources, so their files say nothing about its languages.
const Set<String> _skippedDirectoryNames = <String>{
  'node_modules',
  'build',
  'target',
  'dist',
  'vendor',
};

/// Reads marker candidates from the local filesystem with asynchronous
/// listings, visiting at most the root and one level of subdirectories.
final class LocalWorkspaceMarkerFiles implements WorkspaceMarkerFilesPort {
  const LocalWorkspaceMarkerFiles();

  @override
  Future<Set<String>> markerCandidates(String workspaceRoot) async {
    final names = <String>{};
    final subdirectories = <Directory>[];
    await for (final entry in Directory(
      workspaceRoot,
    ).list(followLinks: false)) {
      final name = p.basename(entry.path);
      if (entry is File) {
        names.add(name);
      } else if (entry is Directory &&
          !name.startsWith('.') &&
          !_skippedDirectoryNames.contains(name.toLowerCase())) {
        subdirectories.add(entry);
      }
    }
    await Future.wait<void>(<Future<void>>[
      for (final directory in subdirectories) _addFileNames(directory, names),
    ]);
    return names;
  }

  static Future<void> _addFileNames(
    Directory directory,
    Set<String> names,
  ) async {
    try {
      await for (final entry in directory.list(followLinks: false)) {
        if (entry is File) names.add(p.basename(entry.path));
      }
    } on FileSystemException {
      // An unreadable subdirectory only hides its own markers.
    }
  }
}
