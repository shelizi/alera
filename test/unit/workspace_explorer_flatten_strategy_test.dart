import 'package:alera/src/features/workbench/application/workspace_explorer_flatten_strategy.dart';
import 'package:alera/src/features/workbench/application/workspace_explorer_node_snapshots.dart';
import 'package:flutter_directory_tree/flutter_directory_tree.dart' as tree;
import 'package:flutter_test/flutter_test.dart';

const _root = 'root';
const _placeholder = WorkspaceExplorerFlattenStrategy.placeholderPrefix;

final class _ReferenceFlatten extends tree.FlattenStrategy {
  const _ReferenceFlatten();

  @override
  List<tree.VisibleNode> flatten({
    required tree.TreeData data,
    required Set<String> expandedIds,
    String? filterQuery,
  }) => const tree.DefaultFlattenStrategy()
      .flatten(data: data, expandedIds: expandedIds, filterQuery: filterQuery)
      .where((row) => !row.id.startsWith(_placeholder))
      .toList(growable: false);
}

tree.TreeNode _node(
  String id,
  String parent, {
  List<String>? children,
  String? name,
  String? virtualPath,
}) => tree.TreeNode(
  id: id,
  name: name ?? id,
  type: id == _root
      ? tree.NodeType.root
      : children != null
      ? tree.NodeType.folder
      : tree.NodeType.file,
  parentId: parent,
  virtualPath: virtualPath ?? '/$id',
  sourcePath: '/repo/$id',
  entryId: children == null ? 'entry:$id' : null,
  childIds: children,
  isVirtual: id.startsWith(_placeholder),
  origin: tree.SelectionOrigin.direct,
);

tree.TreeData _data(Map<String, tree.TreeNode> nodes) => tree.TreeData(
  nodes: nodes,
  rootId: _root,
  visibleRootId: _root,
  omitContainerRowAtRoot: true,
);

Object _fields(tree.VisibleNode row) => (
  row.id,
  row.depth,
  row.name,
  row.type,
  row.hasChildren,
  row.virtualPath,
  row.entryId,
  row.isVirtual,
  row.sourcePath,
  row.origin,
);

void _expectRows(
  List<tree.VisibleNode> actual,
  List<tree.VisibleNode> expected,
) {
  expect(actual.map(_fields).toList(), expected.map(_fields).toList());
}

Map<String, tree.TreeNode> _fixture(int folders, int files) {
  final nodes = <String, tree.TreeNode>{};
  final rootChildren = <String>[];
  for (var i = 0; i < folders; i++) {
    final folder = 'dir-$i';
    rootChildren.add(folder);
    final ids = [for (var j = 0; j < files; j++) '$folder/file-$j.dart'];
    nodes[folder] = _node(folder, _root, children: ids);
    for (final id in ids) {
      nodes[id] = _node(id, folder);
    }
  }
  nodes[_root] = _node(_root, '', children: rootChildren);
  return nodes;
}

final class _Harness {
  _Harness(Map<String, tree.TreeNode> nodes)
    : store = (WorkspaceExplorerNodeSnapshots<tree.TreeNode>()
        ..replace(nodes.entries)) {
    data = _data(store.snapshot());
    strategy.prepareUpdate(data, store.takeChangedKeys());
    controller = tree.DirectoryTreeController(
      data: data,
      flattenStrategy: strategy,
      immutableVisibleRows: true,
      lazyLookupIndexes: true,
      selection: tree.SelectionController(mode: tree.SelectionMode.multi),
    );
    reference = tree.DirectoryTreeController(
      data: data,
      flattenStrategy: const _ReferenceFlatten(),
      lazyLookupIndexes: true,
      selection: tree.SelectionController(mode: tree.SelectionMode.multi),
    );
  }

  final WorkspaceExplorerNodeSnapshots<tree.TreeNode> store;
  final strategy = WorkspaceExplorerFlattenStrategy();
  late tree.TreeData data;
  late final tree.DirectoryTreeController controller;
  late final tree.DirectoryTreeController reference;

  void publish() {
    data = _data(store.snapshot());
    strategy.prepareUpdate(data, store.takeChangedKeys());
    controller.rebuild(data);
    reference.rebuild(data);
    check();
  }

  void expand(Iterable<String> ids) {
    controller.expansions.expandAll(ids);
    reference.expansions.expandAll(ids);
    check();
  }

  void collapse(String id) {
    controller.collapse(id);
    reference.collapse(id);
    check();
  }

  void select(String id) {
    controller.selectOnly(id);
    reference.selectOnly(id);
    check();
  }

  void filter(String query) {
    controller.filterQuery = query;
    reference.filterQuery = query;
    check();
  }

  void check() {
    _expectRows(controller.visibleNodes, reference.visibleNodes);
    expect(controller.expansions.expandedIds, reference.expansions.expandedIds);
    expect(controller.selection.selectedIds, reference.selection.selectedIds);
    for (var i = 0; i < controller.visibleNodes.length; i++) {
      expect(controller.indexOfNode(controller.visibleNodes[i].id), i);
    }
  }

  void dispose() {
    controller.dispose();
    reference.dispose();
  }
}

void main() {
  test(
    'sparse edits reuse unaffected branches and preserve old visible snapshots',
    () {
      final seed = _fixture(6, 60);
      final harness = _Harness(seed);
      addTearDown(harness.dispose);
      harness.expand(['dir-0', 'dir-1', 'dir-2']);
      harness.select('dir-0/file-0.dart');
      final before = harness.controller.visibleNodes;
      final oldRow = before.firstWhere((row) => row.id == 'dir-1/file-12.dart');
      final oldFields = before.map(_fields).toList();

      const removed = 'dir-0/file-0.dart';
      const added = 'dir-0/new.dart';
      harness.store.remove(removed);
      harness.store.put(added, _node(added, 'dir-0', name: 'new.dart'));
      harness.store.put(
        'dir-0',
        _node(
          'dir-0',
          _root,
          children: [added, ...seed['dir-0']!.childIds.skip(1)],
        ),
      );
      harness.publish();
      expect(harness.controller.selection.selectedIds, isEmpty);
      expect(harness.controller.indexOfNode(removed), -1);
      expect(harness.controller.indexOfNode(added), greaterThanOrEqualTo(0));
      expect(
        harness.controller.visibleNodes.firstWhere(
          (row) => row.id == oldRow.id,
        ),
        same(oldRow),
      );
      expect(before.map(_fields).toList(), oldFields);

      final visible = harness.controller.visibleNodes;
      const hidden = 'dir-5/file-1.dart';
      harness.store.put(hidden, _node(hidden, 'dir-5', name: 'changed hidden'));
      harness.publish();
      expect(harness.controller.visibleNodes, same(visible));
      harness.expand(['dir-5']);
      expect(
        harness.controller.visibleNodes
            .firstWhere((row) => row.id == hidden)
            .name,
        'changed hidden',
      );
      harness.collapse('dir-5');
      expect(harness.controller.indexOfNode(hidden), -1);
    },
  );

  test('root reordering, cross-branch move, placeholder and filter fallback retain exact rows', () {
    final seed = _fixture(6, 60);
    const folder = 'dir-0/lazy';
    const placeholder = '${_placeholder}dir-0/lazy';
    seed['dir-0'] = _node(
      'dir-0',
      _root,
      children: [...seed['dir-0']!.childIds, folder],
    );
    seed[folder] = _node(folder, 'dir-0', children: [placeholder]);
    seed[placeholder] = _node(placeholder, folder);
    final harness = _Harness(seed);
    addTearDown(harness.dispose);
    harness.expand(['dir-0', 'dir-1', folder]);
    expect(
      harness.controller.visibleNodes.any((row) => row.id == placeholder),
      isFalse,
    );
    expect(
      harness.controller.visibleNodes
          .firstWhere((row) => row.id == folder)
          .hasChildren,
      isTrue,
    );
    final prior = harness.controller.visibleNodes;
    final stable = prior.firstWhere((row) => row.id == 'dir-1/file-3.dart');
    harness.store.put(
      _root,
      _node(_root, '', children: seed[_root]!.childIds.reversed.toList()),
    );
    harness.publish();
    expect(harness.controller.visibleNodes.first.id, 'dir-5');
    expect(
      harness.controller.visibleNodes.firstWhere((row) => row.id == stable.id),
      same(stable),
    );

    // Move an existing ID between parents. Both old and new branches must
    // be invalidated, including a changed depth.
    const moved = 'dir-1/file-0.dart';
    harness.store.put(
      'dir-1',
      _node('dir-1', _root, children: seed['dir-1']!.childIds.skip(1).toList()),
    );
    harness.store.remove(placeholder);
    harness.store.put(folder, _node(folder, 'dir-0', children: [moved]));
    harness.store.put(
      moved,
      _node(
        moved,
        folder,
        name: 'moved.dart',
        virtualPath: '/dir-0/lazy/moved.dart',
      ),
    );
    harness.publish();
    expect(
      harness.controller.visibleNodes
          .firstWhere((row) => row.id == moved)
          .depth,
      2,
    );
    expect(
      harness.controller.visibleNodes.firstWhere((row) => row.id == moved).name,
      'moved.dart',
    );
    harness.filter('moved');
    harness.filter('');
    harness.collapse('dir-0');
    harness.expand(['dir-0']);

    // Full replacements have no incremental change set.
    harness.store.replace(seed.entries);
    harness.publish();
    expect(harness.controller.visibleNodes.first.id, 'dir-0');
    expect(
      harness.controller.visibleNodes
          .firstWhere((row) => row.id == moved)
          .depth,
      1,
    );
  });

  test('unknown change sets, very wide roots and inconsistent parent links use reference traversal', () {
    final seed = _fixture(6, 60);
    final strategy = WorkspaceExplorerFlattenStrategy();
    final expanded = {'dir-0', 'dir-1'};
    final initial = _data(Map.unmodifiable(seed));
    final before = strategy.flatten(data: initial, expandedIds: expanded);
    final changed = Map<String, tree.TreeNode>.of(seed)
      ..['dir-0/file-0.dart'] = _node(
        'dir-0/file-0.dart',
        'dir-0',
        name: 'updated',
      );
    final next = _data(Map.unmodifiable(changed));
    final actual = strategy.flatten(data: next, expandedIds: expanded);
    _expectRows(
      actual,
      const _ReferenceFlatten().flatten(data: next, expandedIds: expanded),
    );
    expect(
      actual.firstWhere((row) => row.id == 'dir-0/file-0.dart').name,
      'updated',
    );
    expect(
      actual.firstWhere((row) => row.id == 'dir-1/file-0.dart'),
      isNot(same(before.firstWhere((row) => row.id == 'dir-1/file-0.dart'))),
    );

    changed['dir-0/file-0.dart'] = _node(
      'dir-0/file-0.dart',
      'dir-1',
      name: 'ambiguous',
    );
    final ambiguous = _data(Map.unmodifiable(changed));
    strategy.prepareUpdate(ambiguous, {'dir-0/file-0.dart'});
    _expectRows(
      strategy.flatten(data: ambiguous, expandedIds: expanded),
      const _ReferenceFlatten().flatten(data: ambiguous, expandedIds: expanded),
    );

    final wide = _data(Map.unmodifiable(_fixture(2050, 0)));
    _expectRows(
      strategy.flatten(data: wide, expandedIds: expanded),
      const _ReferenceFlatten().flatten(data: wide, expandedIds: expanded),
    );
  });

  test(
    'snapshot handoff tracks compaction, repeated edits and full invalidation',
    () {
      final store = WorkspaceExplorerNodeSnapshots<int>()
        ..replace({for (var i = 0; i < 1000; i++) '$i': i}.entries);
      expect(store.takeChangedKeys(), isNull);
      store.put('10', 2);
      store.put('10', 3);
      store.remove('11');
      store.put('new', 1);
      store.remove('missing');
      store.snapshot();
      expect(store.takeChangedKeys(), {'10', '11', 'new'});
      expect(store.takeChangedKeys(), isEmpty);
      for (var i = 0; i < 300; i++) {
        store.put('added-$i', i);
      }
      store.snapshot(); // Overlay compacts, but all changed keys are retained.
      expect(store.takeChangedKeys(), {
        for (var i = 0; i < 300; i++) 'added-$i',
      });
      for (var i = 0; i < 513; i++) {
        store.put('bulk-$i', i);
      }
      expect(store.takeChangedKeys(), isNull);
      store.clear();
      expect(store.takeChangedKeys(), isNull);
    },
  );

  test(
    'benchmark incremental visible branch publication',
    () {
      final seed = _fixture(200, 200);
      final dirs = seed[_root]!.childIds;
      const updates = 12;

      ({int elapsed, List<Object> rows, int reusedRows}) run(
        bool optimized,
        int expanded,
      ) {
        final store = WorkspaceExplorerNodeSnapshots<tree.TreeNode>()
          ..replace(seed.entries);
        final strategy = WorkspaceExplorerFlattenStrategy();
        final initial = _data(store.snapshot());
        strategy.prepareUpdate(initial, store.takeChangedKeys());
        final controller = tree.DirectoryTreeController(
          data: initial,
          flattenStrategy: optimized ? strategy : const _ReferenceFlatten(),
          immutableVisibleRows: optimized,
          lazyLookupIndexes: true,
        );
        controller.expansions.expandAll(dirs.take(expanded));
        var elapsed = 0;
        var reusedRows = 0;
        for (var i = 0; i < updates; i++) {
          final dir = dirs[i % expanded];
          final oldId = '$dir/file-$i.dart';
          final newId = '$dir/renamed-$i.dart';
          final parent = store.snapshot()[dir]!;
          final oldStable = controller.visibleNodes.last;
          final watch = Stopwatch()..start();
          store.remove(oldId);
          store.put(newId, _node(newId, dir));
          store.put(
            dir,
            parent.copyWith(
              childIds: [
                for (final id in parent.childIds) id == oldId ? newId : id,
              ],
            ),
          );
          final next = _data(store.snapshot());
          strategy.prepareUpdate(next, store.takeChangedKeys());
          controller.rebuild(next);
          watch.stop();
          elapsed += watch.elapsedMicroseconds;
          if (identical(controller.visibleNodes.last, oldStable)) reusedRows++;
        }
        final rows = controller.visibleNodes.map(_fields).toList();
        controller.dispose();
        return (elapsed: elapsed, rows: rows, reusedRows: reusedRows);
      }

      double median(List<int> samples) {
        samples.sort();
        return samples[samples.length ~/ 2] / 1000;
      }

      for (final expanded in [5, 200]) {
        final reference = run(false, expanded);
        final warm = run(true, expanded);
        expect(warm.rows, reference.rows);
        final old = <int>[];
        final next = <int>[];
        for (var sample = 0; sample < 7; sample++) {
          for (final optimized
              in sample.isEven ? [false, true] : [true, false]) {
            final measured = run(optimized, expanded);
            expect(measured.rows, reference.rows);
            expect(measured.reusedRows, optimized ? updates : 0);
            (optimized ? next : old).add(measured.elapsed);
          }
        }
        final oldMs = median(old);
        final newMs = median(next);
        // ignore: avoid_print
        print(
          'Explorer branch flatten expanded=$expanded nodes=${seed.length} visible=${reference.rows.length} updates=$updates old_ms=${oldMs.toStringAsFixed(3)} new_ms=${newMs.toStringAsFixed(3)} speedup=${(oldMs / newMs).toStringAsFixed(2)}x',
        );
      }
    },
    skip: !const bool.fromEnvironment('ALERA_EXPLORER_BENCHMARK'),
    timeout: const Timeout(Duration(minutes: 10)),
  );
}
