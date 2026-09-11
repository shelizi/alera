import 'package:alera/src/features/workbench/application/workbench_closed_tabs_completion_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_closed_tabs_plan.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('active close reads current state, consults MRU, applies layout, then completes', () async {
    final first = _tab('first');
    final second = _tab('second');
    final third = _tab('third');
    final events = <String>[];
    final coordinator = WorkbenchClosedTabsCompletionCoordinator(
      readCurrentTabs: () {
        events.add('read-tabs');
        return <WorkspaceTabRecord>[second, third];
      },
      readCurrentLayout: () {
        events.add('read-layout');
        return _layout(<String>[
          first.id,
          second.id,
          third.id,
        ], activeTabId: second.id);
      },
      mostRecentOpenTabId: (openTabIds) {
        events.add('mru:${openTabIds.toList()..sort()}');
        return third.id;
      },
      applyTabs: (tabs) =>
          events.add('apply-tabs:${tabs.map((tab) => tab.id).join(',')}'),
      forgetFocusHistory: () => events.add('forget-history'),
      applyLayout: (layout) async =>
          events.add('apply-layout:${layout.activeTabId}'),
      completeRemoval: (tabs) =>
          events.add('complete:${tabs.map((tab) => tab.id).join(',')}'),
    );

    await coordinator.run(
      workspaceId: 'workspace',
      snapshot: WorkbenchTabCloseSnapshot(
        closedTabIds: <String>{first.id},
        closingTabs: <String, WorkspaceTabRecord>{first.id: first},
        closedActiveTab: true,
      ),
    );

    expect(events, <String>[
      'read-tabs',
      'mru:[second, third]',
      'read-layout',
      'apply-tabs:second,third',
      'apply-layout:third',
      'complete:second,third',
    ]);
  });

  test(
    'inactive close skips MRU lookup and preserves the current active survivor',
    () async {
      final first = _tab('first');
      final second = _tab('second');
      final events = <String>[];
      final coordinator = WorkbenchClosedTabsCompletionCoordinator(
        readCurrentTabs: () => <WorkspaceTabRecord>[second],
        readCurrentLayout: () =>
            _layout(<String>[first.id, second.id], activeTabId: second.id),
        mostRecentOpenTabId: (_) {
          events.add('mru');
          return second.id;
        },
        applyTabs: (_) => events.add('tabs'),
        forgetFocusHistory: () => events.add('forget'),
        applyLayout: (layout) async =>
            events.add('layout:${layout.activeTabId}'),
        completeRemoval: (_) => events.add('complete'),
      );

      await coordinator.run(
        workspaceId: 'workspace',
        snapshot: WorkbenchTabCloseSnapshot(
          closedTabIds: <String>{first.id},
          closingTabs: <String, WorkspaceTabRecord>{first.id: first},
          closedActiveTab: false,
        ),
      );

      expect(events, <String>['tabs', 'layout:second', 'complete']);
    },
  );

  test('closing the last tab forgets focus history before persisting the empty layout', () async {
    final last = _tab('last');
    final events = <String>[];
    final coordinator = WorkbenchClosedTabsCompletionCoordinator(
      readCurrentTabs: () {
        events.add('read-tabs');
        return const <WorkspaceTabRecord>[];
      },
      readCurrentLayout: () {
        events.add('read-layout');
        return _layout(<String>[last.id], activeTabId: last.id);
      },
      mostRecentOpenTabId: (_) {
        events.add('mru');
        return null;
      },
      applyTabs: (tabs) => events.add('tabs:${tabs.length}'),
      forgetFocusHistory: () => events.add('forget'),
      applyLayout: (layout) async => events.add('layout:${layout.activeTabId}'),
      completeRemoval: (tabs) => events.add('complete:${tabs.length}'),
    );

    await coordinator.run(
      workspaceId: 'workspace',
      snapshot: WorkbenchTabCloseSnapshot(
        closedTabIds: <String>{last.id},
        closingTabs: <String, WorkspaceTabRecord>{last.id: last},
        closedActiveTab: true,
      ),
    );

    expect(events, <String>[
      'read-tabs',
      'read-layout',
      'tabs:0',
      'forget',
      'layout:null',
      'complete:0',
    ]);
  });

  test(
    'filters closed ids from the post-close tab snapshot before planning',
    () async {
      final closed = _tab('closed');
      final survivor = _tab('survivor');
      List<WorkspaceTabRecord>? applied;
      final coordinator = WorkbenchClosedTabsCompletionCoordinator(
        readCurrentTabs: () => <WorkspaceTabRecord>[closed, survivor],
        readCurrentLayout: () =>
            _layout(<String>[closed.id, survivor.id], activeTabId: survivor.id),
        mostRecentOpenTabId: (_) => null,
        applyTabs: (tabs) => applied = tabs,
        forgetFocusHistory: () {},
        applyLayout: (_) async {},
        completeRemoval: (_) {},
      );

      await coordinator.run(
        workspaceId: 'workspace',
        snapshot: WorkbenchTabCloseSnapshot(
          closedTabIds: <String>{closed.id},
          closingTabs: <String, WorkspaceTabRecord>{closed.id: closed},
          closedActiveTab: false,
        ),
      );

      expect(applied?.map((tab) => tab.id), <String>[survivor.id]);
    },
  );

  test('layout failure preserves earlier tab and history changes but skips completion', () async {
    final last = _tab('last');
    final events = <String>[];
    final coordinator = WorkbenchClosedTabsCompletionCoordinator(
      readCurrentTabs: () => const <WorkspaceTabRecord>[],
      readCurrentLayout: () => _layout(<String>[last.id], activeTabId: last.id),
      mostRecentOpenTabId: (_) => null,
      applyTabs: (_) => events.add('tabs'),
      forgetFocusHistory: () => events.add('forget'),
      applyLayout: (_) async {
        events.add('layout');
        throw StateError('layout failed');
      },
      completeRemoval: (_) => events.add('complete'),
    );

    await expectLater(
      coordinator.run(
        workspaceId: 'workspace',
        snapshot: WorkbenchTabCloseSnapshot(
          closedTabIds: <String>{last.id},
          closingTabs: <String, WorkspaceTabRecord>{last.id: last},
          closedActiveTab: true,
        ),
      ),
      throwsA(isA<StateError>()),
    );

    expect(events, <String>['tabs', 'forget', 'layout']);
  });
}

WorkspaceTabRecord _tab(String id) => WorkspaceTabRecord(
  id: id,
  workspaceId: 'workspace',
  title: id,
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
);

WorkbenchLayout _layout(List<String> tabIds, {String? activeTabId}) {
  var layout = WorkbenchLayout.single(workspaceId: 'workspace', tabIds: tabIds);
  if (activeTabId != null) {
    layout = layout.setActiveTab(
      groupId: layout.activeGroupId,
      tabId: activeTabId,
    );
  }
  return layout;
}
