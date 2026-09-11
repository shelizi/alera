import 'package:alera/src/features/workbench/application/workbench_persisted_tab_open_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'restores, persists a missing layout entry, then selects in order',
    () async {
      final events = <String>[];
      final existing = _tab('existing');
      final restored = _tab('restored');
      final coordinator = WorkbenchPersistedTabOpenCoordinator(
        findTab: (tabId) async {
          events.add('find:$tabId');
          return restored;
        },
        isDisposed: () {
          events.add('disposed');
          return false;
        },
        readCurrentTabs: () {
          events.add('read-tabs');
          return <WorkspaceTabRecord>[existing];
        },
        layoutForMutation: (tabs) {
          events.add('layout-for:${tabs.map((tab) => tab.id).join(',')}');
          return _layout(<String>[existing.id]);
        },
        applyTabs: (tabs) =>
            events.add('apply-tabs:${tabs.map((tab) => tab.id).join(',')}'),
        applyLayout: (layout) async => events.add(
          'apply-layout:${layout.groupIdForTab(restored.id) != null}',
        ),
        selectTab: ({required workspaceId, required tabId}) async {
          events.add('select:$workspaceId:$tabId');
        },
      );

      await coordinator.open(workspaceId: 'workspace', tabId: restored.id);

      expect(events, <String>[
        'find:restored',
        'disposed',
        'read-tabs',
        'layout-for:existing',
        'apply-tabs:existing,restored',
        'apply-layout:true',
        'disposed',
        'select:workspace:restored',
      ]);
    },
  );

  test(
    'disposed after lookup returns before validating a missing tab',
    () async {
      final events = <String>[];
      final coordinator = WorkbenchPersistedTabOpenCoordinator(
        findTab: (_) async {
          events.add('find');
          return null;
        },
        isDisposed: () {
          events.add('disposed');
          return true;
        },
        readCurrentTabs: () {
          events.add('read-tabs');
          return const <WorkspaceTabRecord>[];
        },
        layoutForMutation: (_) => _layout(const <String>[]),
        applyTabs: (_) => events.add('tabs'),
        applyLayout: (_) async => events.add('layout'),
        selectTab: ({required workspaceId, required tabId}) async =>
            events.add('select'),
      );

      await coordinator.open(workspaceId: 'workspace', tabId: 'missing');

      expect(events, <String>['find', 'disposed']);
    },
  );

  test(
    'missing or foreign persisted tab fails before local state mutation',
    () async {
      for (final restored in <WorkspaceTabRecord?>[
        null,
        _tab('tab', workspaceId: 'other'),
      ]) {
        final events = <String>[];
        final coordinator = WorkbenchPersistedTabOpenCoordinator(
          findTab: (_) async => restored,
          isDisposed: () => false,
          readCurrentTabs: () {
            events.add('read-tabs');
            return const <WorkspaceTabRecord>[];
          },
          layoutForMutation: (_) => _layout(const <String>[]),
          applyTabs: (_) => events.add('tabs'),
          applyLayout: (_) async => events.add('layout'),
          selectTab: ({required workspaceId, required tabId}) async =>
              events.add('select'),
        );

        await expectLater(
          coordinator.open(workspaceId: 'workspace', tabId: 'tab'),
          throwsA(
            isA<StateError>().having(
              (error) => error.message,
              'message',
              'The created tab is no longer available in this workspace.',
            ),
          ),
        );
        expect(events, isEmpty);
      }
    },
  );

  test(
    'already-local tab refresh skips layout persistence but still selects',
    () async {
      final stale = _tab('tab', title: 'Old');
      final restored = _tab('tab', title: 'Restored');
      final events = <String>[];
      final coordinator = WorkbenchPersistedTabOpenCoordinator(
        findTab: (_) async => restored,
        isDisposed: () => false,
        readCurrentTabs: () => <WorkspaceTabRecord>[stale],
        layoutForMutation: (_) => _layout(<String>[stale.id]),
        applyTabs: (tabs) => events.add('tabs:${tabs.single.title}'),
        applyLayout: (_) async => events.add('layout'),
        selectTab: ({required workspaceId, required tabId}) async =>
            events.add('select:$tabId'),
      );

      await coordinator.open(workspaceId: 'workspace', tabId: restored.id);

      expect(events, <String>['tabs:Restored', 'select:tab']);
    },
  );

  test('disposed after layout persistence skips final selection', () async {
    final restored = _tab('tab');
    var disposed = false;
    final events = <String>[];
    final coordinator = WorkbenchPersistedTabOpenCoordinator(
      findTab: (_) async => restored,
      isDisposed: () => disposed,
      readCurrentTabs: () => const <WorkspaceTabRecord>[],
      layoutForMutation: (_) => _layout(const <String>[]),
      applyTabs: (_) => events.add('tabs'),
      applyLayout: (_) async {
        events.add('layout');
        disposed = true;
      },
      selectTab: ({required workspaceId, required tabId}) async =>
          events.add('select'),
    );

    await coordinator.open(workspaceId: 'workspace', tabId: restored.id);

    expect(events, <String>['tabs', 'layout']);
  });

  test('layout failure keeps applied tabs and prevents selection', () async {
    final restored = _tab('tab');
    final events = <String>[];
    final coordinator = WorkbenchPersistedTabOpenCoordinator(
      findTab: (_) async => restored,
      isDisposed: () => false,
      readCurrentTabs: () => const <WorkspaceTabRecord>[],
      layoutForMutation: (_) => _layout(const <String>[]),
      applyTabs: (_) => events.add('tabs'),
      applyLayout: (_) async {
        events.add('layout');
        throw StateError('layout failed');
      },
      selectTab: ({required workspaceId, required tabId}) async =>
          events.add('select'),
    );

    await expectLater(
      coordinator.open(workspaceId: 'workspace', tabId: restored.id),
      throwsA(isA<StateError>()),
    );

    expect(events, <String>['tabs', 'layout']);
  });
}

WorkspaceTabRecord _tab(
  String id, {
  String workspaceId = 'workspace',
  String? title,
}) => WorkspaceTabRecord(
  id: id,
  workspaceId: workspaceId,
  title: title ?? id,
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
);

WorkbenchLayout _layout(List<String> tabIds) =>
    WorkbenchLayout.single(workspaceId: 'workspace', tabIds: tabIds);
