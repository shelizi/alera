import 'package:alera/src/features/workbench/application/workspace_explorer_projection_index.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:flutter_test/flutter_test.dart';

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
  List<native.WorkspaceFileEntry> entries,
) => native.WorkspaceExplorerDirectoryChildren(
  relativePath: path,
  children: entries,
);

native.WorkspaceExplorerEntryBinding binding(String path, {String? id}) =>
    native.WorkspaceExplorerEntryBinding(
      nodeId: id ?? 'path:$path',
      relativePath: path,
    );

native.WorkspaceExplorerTreeProjection projection(
  List<native.WorkspaceExplorerDirectoryChildren> directories, {
  List<native.WorkspaceExplorerEntryBinding>? bindings,
}) => native.WorkspaceExplorerTreeProjection(
  directories: directories,
  nodes: const [],
  entryBindings:
      bindings ??
      [
        for (final dir in directories)
          for (final row in dir.children) binding(row.relativePath),
      ],
);

native.WorkspaceExplorerProjectionDelta delta({
  List<native.WorkspaceExplorerDirectoryChildren> changed = const [],
  List<String> removed = const [],
  List<native.WorkspaceExplorerEntryBinding> bindings = const [],
  List<String> removedBindings = const [],
}) => native.WorkspaceExplorerProjectionDelta(
  version: BigInt.one,
  changedDirectories: changed,
  removedDirectoryPaths: removed,
  changedNodes: const [],
  removedNodeIds: const [],
  changedBindings: bindings,
  removedBindingNodeIds: removedBindings,
);

/// Independent full-rebuild oracle, as used by the original structural path.
final class FullIndex {
  final directories = <String, List<native.WorkspaceFileEntry>>{};
  final bindings = <String, String>{};
  Map<String, native.WorkspaceFileEntry> byPath = {};
  Map<String, native.WorkspaceFileEntry> byId = {};

  FullIndex(native.WorkspaceExplorerTreeProjection value) {
    for (final dir in value.directories) {
      directories[dir.relativePath] = dir.children;
    }
    for (final link in value.entryBindings) {
      bindings[link.nodeId] = link.relativePath;
    }
    rebuild();
  }

  void apply(native.WorkspaceExplorerProjectionDelta value) {
    for (final path in value.removedDirectoryPaths) {
      directories.remove(path);
    }
    for (final dir in value.changedDirectories) {
      directories[dir.relativePath] = dir.children;
    }
    for (final id in value.removedBindingNodeIds) {
      bindings.remove(id);
    }
    for (final link in value.changedBindings) {
      bindings[link.nodeId] = link.relativePath;
    }
    rebuild();
  }

  void rebuild() {
    final sortedPaths = directories.keys.toList()..sort();
    final sortedIds = bindings.keys.toList()..sort();
    byPath = {
      for (final path in sortedPaths)
        for (final row in directories[path]!) row.relativePath: row,
    };
    byId = {
      for (final id in sortedIds)
        if (byPath[bindings[id]] case final row?) id: row,
    };
  }
}

void expectParity(WorkspaceExplorerProjectionIndex actual, FullIndex expected) {
  expect(actual.childrenByDirectory, expected.directories);
  expect(actual.entryByPath, expected.byPath);
  expect(actual.entryByNodeId, expected.byId);
  expect(
    actual.snapshots().map((dir) => dir.relativePath),
    (expected.directories.keys.toList()..sort()),
  );
}

void main() {
  test(
    'structural and metadata deltas preserve untouched entry identities',
    () {
      final stable = entry('b/keep.txt');
      final initial = projection([
        children('', [
          entry('a', directory: true),
          entry('b', directory: true),
        ]),
        children('a', [entry('a/file.txt'), entry('a/sub', directory: true)]),
        children('a/sub', [entry('a/sub/deep.txt')]),
        children('b', [stable]),
      ]);
      final actual = WorkspaceExplorerProjectionIndex()..replace(initial);
      final expected = FullIndex(initial);
      final updates = [
        delta(
          changed: [
            children('a', [
              entry('a/renamed.txt'),
              entry('a/sub', directory: true),
            ]),
          ],
          bindings: [binding('a/renamed.txt')],
          removedBindings: ['path:a/file.txt'],
        ),
        delta(
          changed: [
            children('a', [
              entry('a/renamed.txt', revision: 2),
              entry('a/sub', directory: true),
            ]),
          ],
        ),
        delta(
          changed: [
            children('a', [entry('a/renamed.txt'), entry('a/sub')]),
          ],
          removed: ['a/sub'],
          removedBindings: ['path:a/sub/deep.txt'],
        ),
        delta(
          changed: [
            children('', [entry('b', directory: true)]),
          ],
          removed: ['a'],
          removedBindings: ['path:a', 'path:a/renamed.txt', 'path:a/sub'],
        ),
        delta(
          changed: [
            children('', [
              entry('b', directory: true),
              entry('empty', directory: true),
            ]),
            children('empty', []),
          ],
          bindings: [binding('empty')],
        ),
        delta(),
      ];
      expectParity(actual, expected);
      for (final update in updates) {
        actual.apply(update);
        expected.apply(update);
        expectParity(actual, expected);
        expect(identical(actual.entryByPath['b/keep.txt'], stable), isTrue);
      }
      expect(
        actual.entryByPath.keys,
        unorderedEquals(['b', 'b/keep.txt', 'empty']),
      );
    },
  );

  test(
    'bindings remain authoritative including unbound and binding-only rows',
    () {
      final initial = projection(
        [
          children('', [entry('file'), entry('unbound')]),
        ],
        bindings: [binding('file')],
      );
      final actual = WorkspaceExplorerProjectionIndex()..replace(initial);
      final expected = FullIndex(initial);
      for (final update in [
        delta(
          changed: [
            children('', [entry('file', revision: 2), entry('unbound')]),
          ],
        ),
        delta(bindings: [binding('unbound')]),
        delta(removedBindings: ['path:file']),
        delta(
          changed: [
            children('', [entry('file', revision: 3), entry('unbound')]),
          ],
        ),
        delta(bindings: [binding('file')]),
      ]) {
        actual.apply(update);
        expected.apply(update);
        expectParity(actual, expected);
      }
      expect(actual.entryByNodeId['path:file']!.size, BigInt.from(3));
    },
  );

  test(
    'ambiguous directories or arbitrary binding IDs use complete indexes',
    () {
      final initial = projection(
        [
          children('', [
            entry('a', directory: true),
            entry('a/file', revision: 1),
          ]),
          children('a', [entry('a/file', revision: 2)]),
        ],
        bindings: [
          binding('a'),
          binding('a/file', id: 'custom-id'),
        ],
      );
      final actual = WorkspaceExplorerProjectionIndex()..replace(initial);
      final expected = FullIndex(initial);
      for (final update in [
        delta(
          changed: [
            children('', [
              entry('a', directory: true),
              entry('a/file', revision: 3),
            ]),
          ],
        ),
        delta(removed: ['a']),
        delta(bindings: [binding('a/file', id: 'another-id')]),
        delta(
          changed: [
            children('', [
              entry('a', directory: true),
              entry('a/file', revision: 4),
            ]),
          ],
        ),
      ]) {
        actual.apply(update);
        expected.apply(update);
        expectParity(actual, expected);
      }
      expect(actual.entryByNodeId['custom-id']!.size, BigInt.from(4));
      expect(actual.entryByNodeId['another-id']!.size, BigInt.from(4));
    },
  );

  test(
    'a noncanonical delta safely leaves incremental mode and reset reseeds',
    () {
      final initial = projection([
        children('', [entry('a', directory: true)]),
        children('a', [entry('a/file', revision: 1)]),
      ]);
      final actual = WorkspaceExplorerProjectionIndex()..replace(initial);
      final expected = FullIndex(initial);
      for (final update in [
        delta(
          changed: [
            children('', [
              entry('a', directory: true),
              entry('a/file', revision: 2),
            ]),
          ],
        ),
        delta(removed: ['a']),
        delta(bindings: [binding('a/file', id: 'alias')]),
      ]) {
        actual.apply(update);
        expected.apply(update);
        expectParity(actual, expected);
      }
      actual.clear();
      expect(actual.entryByPath, isEmpty);
      expect(actual.entryByNodeId, isEmpty);
      expect(actual.snapshots(), isEmpty);
      final next = projection([
        children('', [entry('new.txt')]),
      ]);
      actual.replace(next);
      expectParity(actual, FullIndex(next));
      actual.apply(delta(removedBindings: ['path:new.txt']));
      expect(actual.entryByNodeId, isEmpty);
      expect(actual.entryByPath.keys, ['new.txt']);
    },
  );

  test('cold session delta with an explicitly empty root can seed entries', () {
    final actual = WorkspaceExplorerProjectionIndex();
    final expected = FullIndex(projection([]));
    final updates = [
      delta(changed: [children('', [])]),
      delta(
        changed: [
          children('', [entry('hello')]),
        ],
        bindings: [binding('hello')],
      ),
    ];
    for (final update in updates) {
      actual.apply(update);
      expected.apply(update);
      expectParity(actual, expected);
    }
  });
}
