import 'dart:collection';

import 'package:alera/src/features/workbench/application/workspace_explorer_node_snapshots.dart';

import 'package:flutter_directory_tree/flutter_directory_tree.dart' as tree;
import 'package:flutter_test/flutter_test.dart';

/// Count full-map traversal rather than using timing assertions in tests.
final class _CountingNodes extends MapBase<String, tree.TreeNode> {
  _CountingNodes(Map<String, tree.TreeNode> nodes)
    : _nodes = Map.unmodifiable(nodes);

  final Map<String, tree.TreeNode> _nodes;
  int keyPasses = 0;
  int entryPasses = 0;

  @override
  Iterable<String> get keys sync* {
    keyPasses++;
    yield* _nodes.keys;
  }

  @override
  Iterable<MapEntry<String, tree.TreeNode>> get entries sync* {
    entryPasses++;
    yield* _nodes.entries;
  }

  @override
  tree.TreeNode? operator [](Object? key) => _nodes[key];

  @override
  bool containsKey(Object? key) => _nodes.containsKey(key);

  @override
  int get length => _nodes.length;

  @override
  void operator []=(String key, tree.TreeNode value) =>
      throw UnsupportedError('immutable snapshot');

  @override
  tree.TreeNode? remove(Object? key) =>
      throw UnsupportedError('immutable snapshot');

  @override
  void clear() => throw UnsupportedError('immutable snapshot');
}

tree.TreeData _data(Map<String, tree.TreeNode> nodes) => tree.TreeData(
  nodes: nodes,
  rootId: 'root',
  visibleRootId: 'root',
  omitContainerRowAtRoot: true,
);

tree.TreeNode _file(String id, String parent, String path, {String? entryId}) =>
    tree.TreeNode(
      id: id,
      name: path.split('/').last,
      type: tree.NodeType.file,
      parentId: parent,
      virtualPath: path,
      entryId: entryId ?? id,
    );

tree.TreeNode _folder(String id, List<String> children, {bool root = false}) =>
    tree.TreeNode(
      id: id,
      name: id,
      type: root ? tree.NodeType.root : tree.NodeType.folder,
      parentId: root ? '' : 'root',
      virtualPath: root ? '/' : '/$id',
      childIds: children,
      isExpanded: root,
    );

Map<String, tree.TreeNode> _smallNodes() => {
  'root': _folder('root', ['folder', 'other'], root: true),
  'folder': _folder('folder', ['old', 'duplicate']),
  'other': _folder('other', ['keep']),
  'old': _file('old', 'folder', '/folder/same.txt', entryId: 'shared'),
  'duplicate': _file(
    'duplicate',
    'folder',
    '/folder/same.txt',
    entryId: 'shared',
  ),
  'keep': _file('keep', 'other', '/other/keep.txt'),
};

tree.DirectoryTreeController _controller(
  tree.TreeData data, {
  required bool lazy,
}) => tree.DirectoryTreeController(
  data: data,
  lazyLookupIndexes: lazy,
  selection: tree.SelectionController(mode: tree.SelectionMode.multi),
);

void _expectState(
  tree.DirectoryTreeController actual,
  tree.DirectoryTreeController expected,
) {
  expect(actual.data.nodes, expected.data.nodes);
  expect(
    actual.visibleNodes.map((node) => node.id),
    expected.visibleNodes.map((node) => node.id),
  );
  expect(actual.expansions.expandedIds, expected.expansions.expandedIds);
  expect(actual.selection.selectedIds, expected.selection.selectedIds);
}

void main() {
  test('lazy publication does not enumerate hidden nodes or allocate lookup indexes', () async {
    final initial = _data(Map.unmodifiable(_smallNodes()));
    final eager = _controller(initial, lazy: false);
    final lazy = _controller(initial, lazy: true);
    addTearDown(eager.dispose);
    addTearDown(lazy.dispose);
    final nextNodes = _smallNodes()
      ..remove('old')
      ..['folder'] = _folder('folder', ['duplicate', 'added'])
      ..['added'] = _file('added', 'folder', '/folder/added.txt');
    final eagerNodes = _CountingNodes(nextNodes);
    final lazyNodes = _CountingNodes(nextNodes);
    eager.rebuild(_data(eagerNodes));
    lazy.rebuild(_data(lazyNodes));
    expect(eagerNodes.keyPasses, 1);
    expect(eagerNodes.entryPasses, 2);
    expect(lazyNodes.keyPasses, 0);
    expect(lazyNodes.entryPasses, 0);

    await lazy.reveal(nodeId: 'added', select: true);
    expect(lazy.selection.selectedIds, {'added'});
    expect(lazyNodes.keyPasses, 0);
    expect(lazyNodes.entryPasses, 0);
    expect(lazy.nodeIdForEntryId('added'), 'added');
    expect(lazyNodes.entryPasses, 1);
    expect(lazy.nodeIdForEntryId('missing'), isNull);
    expect(lazy.nodeIdForEntryId('added'), 'added');
    expect(lazyNodes.entryPasses, 1);
    await lazy.reveal(virtualPath: '/other/keep.txt', select: true);
    expect(lazy.selection.selectedIds, {'keep'});
    expect(lazyNodes.entryPasses, 2);
    await lazy.reveal(virtualPath: '/folder/added.txt', select: true);
    expect(lazy.selection.selectedIds, {'added'});
    expect(lazyNodes.entryPasses, 2);
  });

  test('lazy lookup invalidates on replacement and preserves duplicate-key and selection behavior', () async {
    final oldData = _data(Map.unmodifiable(_smallNodes()));
    final eager = _controller(oldData, lazy: false);
    final lazy = _controller(oldData, lazy: true);
    addTearDown(eager.dispose);
    addTearDown(lazy.dispose);

    for (final controller in [eager, lazy]) {
      expect(controller.nodeIdForEntryId('shared'), 'duplicate');
      await controller.reveal(virtualPath: '/folder/same.txt', select: true);
      expect(controller.selection.selectedIds, {'duplicate'});
      controller.expansions.setExpanded('other', true);
    }

    // A lookup from a synchronous selection listener must see the new
    // snapshot too, not stale IDs left by the previous cache.
    String? duringRebuild;
    lazy.selection.addListener(() {
      duringRebuild = lazy.nodeIdForEntryId('shared');
    });
    final nextNodes = _smallNodes()
      ..remove('duplicate')
      ..remove('other')
      ..remove('keep')
      ..['root'] = _folder('root', ['folder'], root: true)
      ..['folder'] = _folder('folder', ['old', 'renamed'])
      ..['renamed'] = _file(
        'renamed',
        'folder',
        '/folder/new.txt',
        entryId: 'renamed-entry',
      );
    final nextData = _data(Map.unmodifiable(nextNodes));
    eager.rebuild(nextData);
    lazy.rebuild(nextData);
    expect(duringRebuild, 'old');
    _expectState(lazy, eager);
    expect(lazy.selection.selectedIds, isEmpty);
    expect(lazy.expansions.expandedIds, isNot(contains('other')));
    expect(lazy.nodeIdForEntryId('shared'), 'old');
    expect(lazy.nodeIdForEntryId('keep'), isNull);
    expect(lazy.nodeIdForEntryId('renamed-entry'), 'renamed');
    for (final controller in [eager, lazy]) {
      await controller.reveal(virtualPath: '/folder/same.txt', select: true);
      expect(controller.selection.selectedIds, {'old'});
      await controller.reveal(virtualPath: '/folder/new.txt', select: true);
      expect(controller.selection.selectedIds, {'renamed'});
      await controller.revealByEntryId('shared', select: true);
      expect(controller.selection.selectedIds, {'old'});
    }
    _expectState(lazy, eager);
    expect(oldData.nodes['duplicate']!.id, 'duplicate');
    expect(oldData.nodes.containsKey('renamed'), isFalse);
    eager.rebuild(oldData, tryPreserveState: false, reseedFromCore: true);
    lazy.rebuild(oldData, tryPreserveState: false, reseedFromCore: true);
    _expectState(lazy, eager);
    expect(lazy.nodeIdForEntryId('shared'), 'duplicate');
  });

  test(
    'shared snapshots keep duplicate lookup order across remove and reinsert',
    () async {
      final nodes = _smallNodes();
      nodes['other'] = _folder('other', [
        'keep',
        for (var i = 0; i < 300; i++) 'hidden-$i',
      ]);
      for (var i = 0; i < 300; i++) {
        nodes['hidden-$i'] = _file('hidden-$i', 'other', '/other/$i');
      }
      final store = WorkspaceExplorerNodeSnapshots<tree.TreeNode>()
        ..replace(nodes.entries);
      final before = store.snapshot();
      final controller = _controller(_data(before), lazy: true);
      addTearDown(controller.dispose);
      expect(controller.nodeIdForEntryId('shared'), 'duplicate');
      store.remove('old');
      store.put('old', nodes['old']!);
      controller.rebuild(_data(store.snapshot()));
      expect(controller.nodeIdForEntryId('shared'), 'old');
      await controller.reveal(virtualPath: '/folder/same.txt', select: true);
      expect(controller.selection.selectedIds, {'old'});
      expect(before.keys.toList(), nodes.keys.toList());
      expect(before['old'], same(nodes['old']));
    },
  );

  test(
    'benchmark shared node snapshots and controller publication',
    () {
      const folderCount = 200;
      const filesPerFolder = 200;
      const updates = 12;
      final folders = [for (var i = 0; i < folderCount; i++) 'folder-$i'];
      final seed = <String, tree.TreeNode>{
        'root': _folder('root', folders, root: true),
      };
      for (final folder in folders) {
        final ids = [
          for (var j = 0; j < filesPerFolder; j++) '$folder/file-$j.dart',
        ];
        seed[folder] = _folder(folder, ids);
        for (final id in ids) {
          seed[id] = _file(id, folder, '/$id');
        }
      }
      final initial = _data(Map.unmodifiable(seed));

      ({int snapshotUs, int totalUs, tree.DirectoryTreeController controller})
      run(bool shared, bool expanded) {
        final controller = _controller(initial, lazy: true);
        controller.expansions.expandAll(expanded ? folders : folders.take(5));
        controller.selectOnly('folder-0/file-1.dart');
        final mutable = Map<String, tree.TreeNode>.of(seed);
        final store = WorkspaceExplorerNodeSnapshots<tree.TreeNode>()
          ..replace(seed.entries);
        var snapshotUs = 0;
        var totalUs = 0;
        for (var i = 0; i < updates; i++) {
          final folder = folders[i];
          final oldId = '$folder/file-0.dart';
          final newId = '$folder/renamed.dart';
          final newFile = _file(newId, folder, '/$newId');
          final newParent = _folder(folder, [
            newId,
            ...seed[folder]!.childIds.skip(1),
          ]);
          final watch = Stopwatch()..start();
          if (shared) {
            store.remove(oldId);
            store.put(newId, newFile);
            store.put(folder, newParent);
          } else {
            mutable.remove(oldId);
            mutable[newId] = newFile;
            mutable[folder] = newParent;
          }
          final snapshot = _data(
            shared
                ? store.snapshot()
                : Map<String, tree.TreeNode>.unmodifiableOf(mutable),
          );
          snapshotUs += watch.elapsedMicroseconds;
          controller.rebuild(snapshot);
          totalUs += watch.elapsedMicroseconds;
          watch.stop();
        }
        return (
          snapshotUs: snapshotUs,
          totalUs: totalUs,
          controller: controller,
        );
      }

      double median(List<int> samples) {
        samples.sort();
        return samples[samples.length ~/ 2] / 1000;
      }

      for (final expanded in [false, true]) {
        final reference = run(false, expanded).controller;
        final warm = run(true, expanded).controller;
        _expectState(warm, reference);
        warm.dispose();
        final oldSnapshots = <int>[];
        final newSnapshots = <int>[];
        final oldTotals = <int>[];
        final newTotals = <int>[];
        for (var sample = 0; sample < 7; sample++) {
          for (final shared in sample.isEven ? [false, true] : [true, false]) {
            final measured = run(shared, expanded);
            (shared ? newSnapshots : oldSnapshots).add(measured.snapshotUs);
            (shared ? newTotals : oldTotals).add(measured.totalUs);
            _expectState(measured.controller, reference);
            expect(
              measured.controller.data.nodes.keys.toList(),
              reference.data.nodes.keys.toList(),
            );
            measured.controller.dispose();
          }
        }
        final rows = reference.visibleNodes.length;
        reference.dispose();
        final before = median(oldSnapshots);
        final after = median(newSnapshots);
        final beforeTotal = median(oldTotals);
        final afterTotal = median(newTotals);
        // ignore: avoid_print
        print(
          'Explorer shared snapshots shape=${expanded ? 'all-expanded' : 'five-expanded'} nodes=${seed.length} visible=$rows updates=$updates old_snapshot_ms=${before.toStringAsFixed(3)} new_snapshot_ms=${after.toStringAsFixed(3)} snapshot_speedup=${(before / after).toStringAsFixed(2)}x old_total_ms=${beforeTotal.toStringAsFixed(3)} new_total_ms=${afterTotal.toStringAsFixed(3)} total_speedup=${(beforeTotal / afterTotal).toStringAsFixed(2)}x',
        );
      }
    },
    skip: !const bool.fromEnvironment('ALERA_EXPLORER_BENCHMARK'),
    timeout: const Timeout(Duration(minutes: 10)),
  );
  test(
    'benchmark large tree structural snapshot publication',
    () {
      const folderCount = 200;
      const filesPerFolder = 200;
      const updates = 12;
      final folders = [for (var i = 0; i < folderCount; i++) 'folder-$i'];
      final seed = <String, tree.TreeNode>{
        'root': _folder('root', folders, root: true),
      };
      for (final folder in folders) {
        final ids = [
          for (var j = 0; j < filesPerFolder; j++) '$folder/file-$j.dart',
        ];
        seed[folder] = _folder(folder, ids);
        for (final id in ids) {
          seed[id] = _file(id, folder, '/$id');
        }
      }
      final initial = _data(Map.unmodifiable(seed));

      ({int snapshotUs, int rebuildUs, tree.DirectoryTreeController controller})
      run(bool lazy, bool expanded) {
        final controller = _controller(initial, lazy: lazy);
        if (expanded) {
          controller.expansions.expandAll(folders);
        } else {
          controller.expansions.expandAll(folders.take(5));
        }
        controller.selectOnly('folder-0/file-1.dart');
        final mutable = Map<String, tree.TreeNode>.of(seed);
        var snapshotUs = 0;
        var rebuildUs = 0;
        for (var i = 0; i < updates; i++) {
          final folder = folders[i];
          final oldId = '$folder/file-0.dart';
          final newId = '$folder/renamed.dart';
          mutable.remove(oldId);
          mutable[newId] = _file(newId, folder, '/$newId');
          mutable[folder] = _folder(folder, [
            newId,
            ...mutable[folder]!.childIds.skip(1),
          ]);
          final watch = Stopwatch()..start();
          final snapshot = _data(
            Map<String, tree.TreeNode>.unmodifiableOf(mutable),
          );
          watch.stop();
          snapshotUs += watch.elapsedMicroseconds;
          watch
            ..reset()
            ..start();
          controller.rebuild(snapshot);
          watch.stop();
          rebuildUs += watch.elapsedMicroseconds;
        }
        return (
          snapshotUs: snapshotUs,
          rebuildUs: rebuildUs,
          controller: controller,
        );
      }

      double median(List<int> samples) {
        samples.sort();
        return samples[samples.length ~/ 2] / 1000;
      }

      for (final expanded in [false, true]) {
        final reference = run(false, expanded).controller;
        final warm = run(true, expanded).controller;
        _expectState(warm, reference);
        warm.dispose();
        final eager = <int>[];
        final lazy = <int>[];
        final eagerSnapshots = <int>[];
        final lazySnapshots = <int>[];
        for (var sample = 0; sample < 7; sample++) {
          for (final optimized
              in sample.isEven ? [false, true] : [true, false]) {
            final measured = run(optimized, expanded);
            (optimized ? lazy : eager).add(measured.rebuildUs);
            (optimized ? lazySnapshots : eagerSnapshots).add(
              measured.snapshotUs,
            );
            _expectState(measured.controller, reference);
            measured.controller.dispose();
          }
        }
        expect(initial.nodes.containsKey('folder-0/file-0.dart'), isTrue);
        expect(initial.nodes.containsKey('folder-0/renamed.dart'), isFalse);
        final rows = reference.visibleNodes.length;
        reference.dispose();
        final oldMs = median(eager);
        final newMs = median(lazy);
        final oldSnapshotMs = median(eagerSnapshots);
        final newSnapshotMs = median(lazySnapshots);
        // ignore: avoid_print
        print(
          'Explorer publication shape=${expanded ? 'all-expanded' : 'five-expanded'} nodes=${seed.length} visible=$rows updates=$updates eager_ms=${oldMs.toStringAsFixed(3)} lazy_ms=${newMs.toStringAsFixed(3)} speedup=${(oldMs / newMs).toStringAsFixed(2)}x snapshot_eager_ms=${oldSnapshotMs.toStringAsFixed(3)} snapshot_lazy_ms=${newSnapshotMs.toStringAsFixed(3)}',
        );
      }
    },
    skip: !const bool.fromEnvironment('ALERA_EXPLORER_BENCHMARK'),
    timeout: const Timeout(Duration(minutes: 10)),
  );
}
