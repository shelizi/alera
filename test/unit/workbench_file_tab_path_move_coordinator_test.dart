import 'package:alera/src/features/workbench/application/workbench_file_tab_path_move_coordinator.dart';
import 'package:alera/src/features/workbench/application/workspace_tab_service.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'empty store result leaves tabs, layout, and completion untouched',
    () async {
      final events = <String>[];
      final coordinator = WorkbenchFileTabPathMoveCoordinator(
        movePaths:
            ({
              required workspaceId,
              required oldRelativePath,
              required newRelativePath,
            }) async {
              events.add('move:$workspaceId:$oldRelativePath:$newRelativePath');
              return const WorkspaceFileTabPathMoveResult(
                updatedTabs: <WorkspaceTabRecord>[],
                closedTabIds: <String>[],
              );
            },
        readCurrentTabs: () => <WorkspaceTabRecord>[_tab('tab', 'old.md')],
        readCurrentLayout: () => _layout(<String>['tab']),
        applyTabs: (_) => events.add('tabs'),
        applyLayout: (_) async => events.add('layout'),
        completeRemoval: (_) => events.add('complete'),
      );

      await coordinator.run(
        workspaceId: 'workspace',
        oldRelativePath: 'old.md',
        newRelativePath: 'new.md',
      );

      expect(events, <String>['move:workspace:old.md:new.md']);
    },
  );

  test(
    'updated records apply tabs and completion without persisting layout',
    () async {
      final events = <String>[];
      final current = _tab('tab', 'old.md');
      final updated = _tab('tab', 'new.md');
      final coordinator = WorkbenchFileTabPathMoveCoordinator(
        movePaths:
            ({
              required workspaceId,
              required oldRelativePath,
              required newRelativePath,
            }) async => WorkspaceFileTabPathMoveResult(
              updatedTabs: <WorkspaceTabRecord>[updated],
              closedTabIds: const <String>[],
            ),
        readCurrentTabs: () => <WorkspaceTabRecord>[current],
        readCurrentLayout: () => _layout(<String>[current.id]),
        applyTabs: (tabs) => events.add('tabs:${tabs.single.filePath}'),
        applyLayout: (_) async => events.add('layout'),
        completeRemoval: (tabs) => events.add('complete:${tabs.single.id}'),
      );

      await coordinator.run(
        workspaceId: 'workspace',
        oldRelativePath: 'old.md',
        newRelativePath: 'new.md',
      );

      expect(events, <String>['tabs:new.md', 'complete:tab']);
    },
  );

  test(
    'closed records apply tabs, persist layout, then complete removal in order',
    () async {
      final events = <String>[];
      final keep = _tab('keep', 'keep.md');
      final closed = _tab('closed', 'old.md');
      final coordinator = WorkbenchFileTabPathMoveCoordinator(
        movePaths:
            ({
              required workspaceId,
              required oldRelativePath,
              required newRelativePath,
            }) async => const WorkspaceFileTabPathMoveResult(
              updatedTabs: <WorkspaceTabRecord>[],
              closedTabIds: <String>['closed'],
            ),
        readCurrentTabs: () => <WorkspaceTabRecord>[keep, closed],
        readCurrentLayout: () => _layout(<String>[keep.id, closed.id]),
        applyTabs: (tabs) =>
            events.add('tabs:${tabs.map((tab) => tab.id).join(',')}'),
        applyLayout: (layout) async {
          events.add('layout:${layout.groupIdForTab(closed.id) == null}');
        },
        completeRemoval: (tabs) => events.add('complete:${tabs.single.id}'),
      );

      await coordinator.run(
        workspaceId: 'workspace',
        oldRelativePath: 'old.md',
        newRelativePath: 'new.txt',
      );

      expect(events, <String>['tabs:keep', 'layout:true', 'complete:keep']);
    },
  );

  test(
    'reads the current snapshot only after the store mutation completes',
    () async {
      final before = _tab('before', 'before.md');
      final after = _tab('after', 'after.md');
      var currentTabs = <WorkspaceTabRecord>[before];
      var currentLayout = _layout(<String>[before.id]);
      final applied = <WorkspaceTabRecord>[];
      final coordinator = WorkbenchFileTabPathMoveCoordinator(
        movePaths:
            ({
              required workspaceId,
              required oldRelativePath,
              required newRelativePath,
            }) async {
              currentTabs = <WorkspaceTabRecord>[after];
              currentLayout = _layout(<String>[after.id]);
              return WorkspaceFileTabPathMoveResult(
                updatedTabs: <WorkspaceTabRecord>[_tab('after', 'renamed.md')],
                closedTabIds: const <String>[],
              );
            },
        readCurrentTabs: () => currentTabs,
        readCurrentLayout: () => currentLayout,
        applyTabs: applied.addAll,
        applyLayout: (_) async {},
        completeRemoval: (_) {},
      );

      await coordinator.run(
        workspaceId: 'workspace',
        oldRelativePath: 'after.md',
        newRelativePath: 'renamed.md',
      );

      expect(applied.map((tab) => tab.id), <String>['after']);
      expect(applied.single.filePath, 'renamed.md');
    },
  );

  test('store failure stops every state callback and propagates', () async {
    final events = <String>[];
    final coordinator = WorkbenchFileTabPathMoveCoordinator(
      movePaths:
          ({
            required workspaceId,
            required oldRelativePath,
            required newRelativePath,
          }) async {
            events.add('move');
            throw StateError('move failed');
          },
      readCurrentTabs: () => const <WorkspaceTabRecord>[],
      readCurrentLayout: () => null,
      applyTabs: (_) => events.add('tabs'),
      applyLayout: (_) async => events.add('layout'),
      completeRemoval: (_) => events.add('complete'),
    );

    await expectLater(
      coordinator.run(
        workspaceId: 'workspace',
        oldRelativePath: 'old.md',
        newRelativePath: 'new.md',
      ),
      throwsA(isA<StateError>()),
    );
    expect(events, <String>['move']);
  });

  test(
    'layout failure happens after tabs apply and prevents removal completion',
    () async {
      final events = <String>[];
      final closed = _tab('closed', 'old.md');
      final coordinator = WorkbenchFileTabPathMoveCoordinator(
        movePaths:
            ({
              required workspaceId,
              required oldRelativePath,
              required newRelativePath,
            }) async => const WorkspaceFileTabPathMoveResult(
              updatedTabs: <WorkspaceTabRecord>[],
              closedTabIds: <String>['closed'],
            ),
        readCurrentTabs: () => <WorkspaceTabRecord>[closed],
        readCurrentLayout: () => _layout(<String>[closed.id]),
        applyTabs: (_) => events.add('tabs'),
        applyLayout: (_) async {
          events.add('layout');
          throw StateError('layout failed');
        },
        completeRemoval: (_) => events.add('complete'),
      );

      await expectLater(
        coordinator.run(
          workspaceId: 'workspace',
          oldRelativePath: 'old.md',
          newRelativePath: 'new.txt',
        ),
        throwsA(isA<StateError>()),
      );
      expect(events, <String>['tabs', 'layout']);
    },
  );
}

WorkspaceTabRecord _tab(String id, String path) => WorkspaceTabRecord(
  id: id,
  workspaceId: 'workspace',
  title: id,
  kind: WorkspaceTabKind.editor,
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
  payload: <String, Object?>{workspaceTabFilePathPayloadKey: path},
);

WorkbenchLayout _layout(List<String> tabIds) =>
    WorkbenchLayout.single(workspaceId: 'workspace', tabIds: tabIds);
