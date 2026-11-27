// ignore_for_file: avoid_print

/// Synthetic Dart-side entry-index benchmark. Does not call the filesystem,
/// native projection, watcher, or Flutter tree rendering.
library;

import 'package:alera/src/features/workbench/application/workspace_explorer_projection_index.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;

native.WorkspaceFileEntry entry(
  String path, {
  bool directory = false,
  int revision = 1,
}) => native.WorkspaceFileEntry(
  relativePath: path,
  name: path.split('/').last,
  kind: directory
      ? native.WorkspaceFileKind.directory
      : native.WorkspaceFileKind.file,
  size: BigInt.from(revision),
  modifiedMillis: revision,
  contentToken: 'revision-$revision',
  isIgnored: false,
  isHidden: false,
  isSymlink: false,
  isProtected: false,
  hasChildrenHint: directory,
);

native.WorkspaceExplorerDirectoryChildren children(
  String path,
  List<native.WorkspaceFileEntry> rows,
) => native.WorkspaceExplorerDirectoryChildren(
  relativePath: path,
  children: rows,
);

native.WorkspaceExplorerEntryBinding binding(String path) =>
    native.WorkspaceExplorerEntryBinding(
      nodeId: 'path:$path',
      relativePath: path,
    );

/// Conservative baseline: only rebuild the three entry indexes from the
/// resulting directory/binding lists, without charging the former native
/// DTO node reconstruction and sorting work.
final class FullEntryIndex {
  final directories = <String, native.WorkspaceExplorerDirectoryChildren>{};
  final bindings = <String, native.WorkspaceExplorerEntryBinding>{};
  Map<String, native.WorkspaceFileEntry> byPath = {};
  Map<String, native.WorkspaceFileEntry> byId = {};

  FullEntryIndex(native.WorkspaceExplorerTreeProjection value) {
    for (final item in value.directories) {
      directories[item.relativePath] = item;
    }
    for (final item in value.entryBindings) {
      bindings[item.nodeId] = item;
    }
    rebuild();
  }

  void apply(native.WorkspaceExplorerProjectionDelta value) {
    for (final path in value.removedDirectoryPaths) {
      directories.remove(path);
    }
    for (final item in value.changedDirectories) {
      directories[item.relativePath] = item;
    }
    for (final id in value.removedBindingNodeIds) {
      bindings.remove(id);
    }
    for (final item in value.changedBindings) {
      bindings[item.nodeId] = item;
    }
    rebuild();
  }

  void rebuild() {
    final orderedDirectories = directories.values.toList()
      ..sort((a, b) => a.relativePath.compareTo(b.relativePath));
    final orderedBindings = bindings.values.toList()
      ..sort((a, b) => a.nodeId.compareTo(b.nodeId));
    byPath = {
      for (final dir in orderedDirectories)
        for (final row in dir.children) row.relativePath: row,
    };
    byId = {
      for (final link in orderedBindings)
        if (byPath[link.relativePath] case final row?) link.nodeId: row,
    };
  }
}

void requireParity(
  Map<String, native.WorkspaceFileEntry> actual,
  Map<String, native.WorkspaceFileEntry> expected,
) {
  if (actual.length != expected.length)
    throw StateError('Entry count mismatch');
  for (final item in expected.entries) {
    if (!identical(actual[item.key], item.value)) {
      throw StateError('Entry mismatch: ${item.key}');
    }
  }
}

void main() {
  const folders = 200;
  const filesPerFolder = 200;
  const updates = 30;
  final paths = List.generate(
    folders,
    (i) => 'folder-${i.toString().padLeft(3, '0')}',
  );
  final directories = [
    children('', [for (final path in paths) entry(path, directory: true)]),
    for (final path in paths)
      children(path, [
        for (var i = 0; i < filesPerFolder; i++) entry('$path/file-$i.txt'),
      ]),
  ];
  final seed = native.WorkspaceExplorerTreeProjection(
    directories: directories,
    nodes: const [],
    entryBindings: [
      for (final dir in directories)
        for (final row in dir.children) binding(row.relativePath),
    ],
  );
  final changes = <native.WorkspaceExplorerProjectionDelta>[];
  for (var i = 0; i < updates; i++) {
    // Rename a single file in a different already-loaded folder each time.
    // Include metadata-only replacements in the structural workload only
    // when they also carry an actual added/removed binding.
    final dir = directories[i + 1];
    final oldPath = dir.children[0].relativePath;
    final newPath = '${dir.relativePath}/renamed.txt';
    changes.add(
      native.WorkspaceExplorerProjectionDelta(
        version: BigInt.from(i + 1),
        changedDirectories: [
          children(dir.relativePath, [
            entry(newPath, revision: 2),
            ...dir.children.skip(1),
          ]),
        ],
        removedDirectoryPaths: const [],
        changedNodes: const [],
        removedNodeIds: const [],
        changedBindings: [binding(newPath)],
        removedBindingNodeIds: ['path:$oldPath'],
      ),
    );
  }

  final expected = FullEntryIndex(seed);
  final warm = WorkspaceExplorerProjectionIndex()..replace(seed);
  for (final change in changes) {
    expected.apply(change);
    warm.apply(change);
    requireParity(warm.entryByPath, expected.byPath);
    requireParity(warm.entryByNodeId, expected.byId);
  }

  final oldSamples = <int>[];
  final newSamples = <int>[];
  for (var sample = 0; sample < 7; sample++) {
    for (final optimized in sample.isEven ? [false, true] : [true, false]) {
      // Seed cost is excluded from both policies.
      if (optimized) {
        final index = WorkspaceExplorerProjectionIndex()..replace(seed);
        final timer = Stopwatch()..start();
        for (final change in changes) {
          index.apply(change);
        }
        timer.stop();
        newSamples.add(timer.elapsedMicroseconds);
        requireParity(index.entryByPath, expected.byPath);
        requireParity(index.entryByNodeId, expected.byId);
      } else {
        final index = FullEntryIndex(seed);
        final timer = Stopwatch()..start();
        for (final change in changes) {
          index.apply(change);
        }
        timer.stop();
        oldSamples.add(timer.elapsedMicroseconds);
        requireParity(index.byPath, expected.byPath);
        requireParity(index.byId, expected.byId);
      }
    }
  }
  double median(List<int> values) {
    values.sort();
    return values[values.length ~/ 2] / 1000;
  }

  final oldMs = median(oldSamples);
  final newMs = median(newSamples);
  print(
    'Classic entry-index structural batch folders=$folders entries=${expected.byPath.length} updates=$updates old_ms=${oldMs.toStringAsFixed(3)} new_ms=${newMs.toStringAsFixed(3)} speedup=${(oldMs / newMs).toStringAsFixed(2)}x',
  );
  print(
    'Each update replaces $filesPerFolder direct children; all map entries and object identities match the full-rebuild oracle.',
  );
}
