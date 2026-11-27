import 'package:alera/src/features/workbench/application/workspace_explorer_controller_state.dart';
import 'package:alera/src/features/workbench/application/workspace_explorer_projection_index.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:flutter_directory_tree/flutter_directory_tree.dart' as tree;
import 'package:flutter_test/flutter_test.dart';

import 'workspace_explorer_projection_index_test.dart' as fixtures;

final class _Fixture {
  _Fixture(int folderCount, int fileCount) {
    final folders = [for (var i = 0; i < folderCount; i++) 'folder-$i'];
    final directories = [
      fixtures.children('', [
        for (final folder in folders) fixtures.entry(folder, directory: true),
      ]),
      for (final folder in folders)
        fixtures.children(folder, [
          for (var j = 0; j < fileCount; j++)
            fixtures.entry('$folder/file-$j.dart'),
        ]),
    ];
    index.replace(fixtures.projection(directories));
    expandedPaths = folders.toSet();
    selectedPath = '${folders.last}/file-${fileCount - 1}.dart';
    final nodes = <String, tree.TreeNode>{
      'workspace-root': tree.TreeNode(
        id: 'workspace-root',
        name: 'repo',
        type: tree.NodeType.root,
        parentId: '',
        virtualPath: '/',
        sourcePath: '/repo',
        isExpanded: true,
        childIds: [for (final folder in folders) 'path:$folder'],
      ),
    };
    for (final directory in directories) {
      final parentId = directory.relativePath.isEmpty
          ? 'workspace-root'
          : 'path:${directory.relativePath}';
      for (final row in directory.children) {
        final path = row.relativePath;
        final id = 'path:$path';
        nodes[id] = tree.TreeNode(
          id: id,
          name: row.name,
          type: row.kind == native.WorkspaceFileKind.directory
              ? tree.NodeType.folder
              : tree.NodeType.file,
          parentId: parentId,
          virtualPath: '/$path',
          sourcePath: path,
          entryId: path,
          childIds: [
            for (final child
                in index.childrenByDirectory[path] ??
                    const <native.WorkspaceFileEntry>[])
              'path:${child.relativePath}',
          ],
        );
      }
    }
    data = tree.TreeData(
      nodes: Map.unmodifiable(nodes),
      rootId: 'workspace-root',
      visibleRootId: 'workspace-root',
      omitContainerRowAtRoot: true,
    );
  }

  final index = WorkspaceExplorerProjectionIndex();
  late final tree.TreeData data;
  late final Set<String> expandedPaths;
  late final String selectedPath;

  tree.DirectoryTreeController controller() => tree.DirectoryTreeController(
    data: data,
    selection: tree.SelectionController(mode: tree.SelectionMode.multi),
  );

  void restorePrevious(tree.DirectoryTreeController controller) {
    for (final entry in index.entryByNodeId.entries) {
      final path = entry.value.relativePath;
      if (expandedPaths.contains(path)) {
        controller.expansions.setExpanded(entry.key, true);
      }
      if (path == selectedPath) controller.selection.selectOnly(entry.key);
    }
  }

  void restoreBatched(tree.DirectoryTreeController controller) {
    restoreWorkspaceExplorerControllerState(
      controller: controller,
      index: index,
      expandedPaths: expandedPaths,
      selectedPath: selectedPath,
    );
  }
}

void _expectControllerParity(
  tree.DirectoryTreeController actual,
  tree.DirectoryTreeController expected,
) {
  expect(actual.expansions.expandedIds, expected.expansions.expandedIds);
  expect(actual.selection.selectedIds, expected.selection.selectedIds);
  expect(
    actual.visibleNodes.map((node) => node.id),
    expected.visibleNodes.map((node) => node.id),
  );
}

void main() {
  test(
    'restore expands all saved folders with one visible-list recomputation',
    () {
      final fixture = _Fixture(20, 10);
      final old = fixture.controller();
      final next = fixture.controller();
      addTearDown(old.dispose);
      addTearDown(next.dispose);
      var oldNotifications = 0;
      var nextNotifications = 0;
      old.expansions.addListener(() => oldNotifications++);
      next.expansions.addListener(() => nextNotifications++);
      fixture.restorePrevious(old);
      fixture.restoreBatched(next);
      _expectControllerParity(next, old);
      expect(oldNotifications, 20);
      expect(nextNotifications, 1);
      final visible = next.visibleNodes;
      fixture.restoreBatched(next);
      expect(nextNotifications, 1);
      expect(next.visibleNodes, same(visible));
    },
  );

  test('select all uses one notification and preserves visible order and selection', () {
    final fixture = _Fixture(8, 10);
    final controller = fixture.controller();
    addTearDown(controller.dispose);
    fixture.restoreBatched(controller);
    final rows = controller.visibleNodes;
    var selectionNotifications = 0;
    controller.selection.addListener(() => selectionNotifications++);
    selectAllWorkspaceExplorerNodes(controller, rows);
    expect(selectionNotifications, 1);
    expect(controller.selection.selectedIds, rows.map((row) => row.id).toSet());
    expect(controller.selection.selectedIds.first, rows.first.id);
    expect(controller.visibleNodes, same(rows));
  });

  test(
    'canonical lookups and noncanonical aliases preserve binding semantics',
    () {
      final index = WorkspaceExplorerProjectionIndex()
        ..replace(
          fixtures.projection(
            [
              fixtures.children('', [
                fixtures.entry('a', directory: true),
                fixtures.entry('unbound'),
              ]),
            ],
            bindings: [fixtures.binding('a')],
          ),
        );
      expect(index.boundNodeIdsForPaths({'a', 'unbound', 'missing'}), [
        'path:a',
      ]);
      expect(index.firstBoundNodeIdForPath('a'), 'path:a');
      expect(index.firstBoundNodeIdForPath('unbound'), isNull);
      index.apply(
        fixtures.delta(bindings: [fixtures.binding('a', id: 'alias')]),
      );
      expect(index.boundNodeIdsForPaths({'a'}), ['alias', 'path:a']);
      expect(index.firstBoundNodeIdForPath('a'), 'alias');
      index.apply(fixtures.delta(removedBindings: ['alias']));
      expect(index.firstBoundNodeIdForPath('a'), 'path:a');
      index.apply(fixtures.delta(removedBindings: ['path:a']));
      expect(index.boundNodeIdsForPaths({'a'}), isEmpty);
    },
  );

  test(
    'benchmark controller restoration on 40,200 rows',
    () {
      final fixture = _Fixture(200, 200);
      final reference = fixture.controller();
      fixture.restorePrevious(reference);
      final warm = fixture.controller();
      fixture.restoreBatched(warm);
      _expectControllerParity(warm, reference);
      warm.dispose();

      final old = <int>[];
      final next = <int>[];
      for (var sample = 0; sample < 7; sample++) {
        for (final batched in sample.isEven ? [false, true] : [true, false]) {
          final controller = fixture.controller();
          final watch = Stopwatch()..start();
          if (batched) {
            fixture.restoreBatched(controller);
          } else {
            fixture.restorePrevious(controller);
          }
          watch.stop();
          (batched ? next : old).add(watch.elapsedMicroseconds);
          _expectControllerParity(controller, reference);
          controller.dispose();
        }
      }
      reference.dispose();
      old.sort();
      next.sort();
      final oldMs = old[old.length ~/ 2] / 1000;
      final newMs = next[next.length ~/ 2] / 1000;
      // ignore: avoid_print
      print(
        'Explorer controller restore folders=200 files=40000 old_ms=${oldMs.toStringAsFixed(3)} new_ms=${newMs.toStringAsFixed(3)} speedup=${(oldMs / newMs).toStringAsFixed(2)}x',
      );
    },
    skip: !const bool.fromEnvironment('ALERA_EXPLORER_BENCHMARK'),
    timeout: const Timeout(Duration(minutes: 10)),
  );
}
