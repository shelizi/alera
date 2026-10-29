import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_workspace_selection_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_selection_hydrator.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('a slower first hydration must not replace a newer same-workspace selection', () async {
    final generation = WorkbenchWorkspaceSelectionEpoch();
    final firstGate = Completer<WorkbenchWorkspaceSelectionHydration>();
    final calls = <String>[];
    final firstToken = generation.begin();
    final first = WorkbenchWorkspaceSelectionCoordinator(
      activateSelection: () => calls.add('activate:first'),
      hydrate: ({required workspaceId, required ensureInitialTerminal}) =>
          firstGate.future,
      applyTabs: (_) => calls.add('tabs:first'),
      applyLayout: (_) async => calls.add('layout:first'),
      isSelectionCurrent: () => generation.isCurrent(firstToken),
      recordHistory: () {
        calls.add('history:first');
        return true;
      },
      notifyHistoryChanged: () => calls.add('notify:first'),
    );
    final pending = first.select(
      workspaceId: 'workspace',
      ensureInitialTerminal: false,
      shouldRecordHistory: true,
    );
    final secondToken = generation.begin();
    final second = WorkbenchWorkspaceSelectionCoordinator(
      activateSelection: () => calls.add('activate:second'),
      hydrate: ({required workspaceId, required ensureInitialTerminal}) async =>
          WorkbenchWorkspaceSelectionHydration(
            tabs: <WorkspaceTabRecord>[_tab('new')],
            layout: _layout(<String>['new']),
          ),
      applyTabs: (_) => calls.add('tabs:second'),
      applyLayout: (_) async => calls.add('layout:second'),
      isSelectionCurrent: () => generation.isCurrent(secondToken),
      recordHistory: () {
        calls.add('history:second');
        return true;
      },
      notifyHistoryChanged: () => calls.add('notify:second'),
    );
    await second.select(
      workspaceId: 'workspace',
      ensureInitialTerminal: false,
      shouldRecordHistory: true,
    );
    firstGate.complete(
      WorkbenchWorkspaceSelectionHydration(
        tabs: <WorkspaceTabRecord>[_tab('old')],
        layout: _layout(<String>['old']),
      ),
    );
    await pending;
    expect(calls, <String>[
      'activate:first',
      'activate:second',
      'tabs:second',
      'layout:second',
      'history:second',
      'notify:second',
    ]);
  });

  test(
    'activates, hydrates, applies tabs and layout, then records history',
    () async {
      final events = <String>[];
      final tabs = <WorkspaceTabRecord>[_tab('tab')];
      final layout = _layout(<String>['tab']);
      final coordinator = WorkbenchWorkspaceSelectionCoordinator(
        activateSelection: () => events.add('activate'),
        hydrate:
            ({required workspaceId, required ensureInitialTerminal}) async {
              events.add('hydrate:$workspaceId:$ensureInitialTerminal');
              return WorkbenchWorkspaceSelectionHydration(
                tabs: tabs,
                layout: layout,
              );
            },
        applyTabs: (tabs) => events.add('tabs:${tabs.single.id}'),
        applyLayout: (layout) async =>
            events.add('layout:${layout.activeTabId}'),
        isSelectionCurrent: () => true,
        recordHistory: () {
          events.add('history');
          return true;
        },
        notifyHistoryChanged: () => events.add('notify'),
      );

      await coordinator.select(
        workspaceId: 'workspace',
        ensureInitialTerminal: true,
        shouldRecordHistory: true,
      );

      expect(events, <String>[
        'activate',
        'hydrate:workspace:true',
        'tabs:tab',
        'layout:tab',
        'history',
        'notify',
      ]);
    },
  );

  test('disabled history does not call the history recorder', () async {
    final events = <String>[];
    final coordinator = _coordinator(events: events, recordHistoryResult: true);

    await coordinator.select(
      workspaceId: 'workspace',
      ensureInitialTerminal: false,
      shouldRecordHistory: false,
    );

    expect(events, <String>['activate', 'hydrate:false', 'tabs', 'layout']);
  });

  test('unchanged history skips the history notification', () async {
    final events = <String>[];
    final coordinator = _coordinator(
      events: events,
      recordHistoryResult: false,
    );

    await coordinator.select(
      workspaceId: 'workspace',
      ensureInitialTerminal: false,
      shouldRecordHistory: true,
    );

    expect(events, <String>[
      'activate',
      'hydrate:false',
      'tabs',
      'layout',
      'history',
    ]);
  });

  test(
    'hydration failure leaves the initial selection applied and stops',
    () async {
      final events = <String>[];
      final coordinator = WorkbenchWorkspaceSelectionCoordinator(
        activateSelection: () => events.add('activate'),
        hydrate:
            ({required workspaceId, required ensureInitialTerminal}) async {
              events.add('hydrate');
              throw StateError('hydrate failed');
            },
        applyTabs: (_) => events.add('tabs'),
        applyLayout: (_) async => events.add('layout'),
        isSelectionCurrent: () => true,
        recordHistory: () {
          events.add('history');
          return true;
        },
        notifyHistoryChanged: () => events.add('notify'),
      );

      await expectLater(
        coordinator.select(
          workspaceId: 'workspace',
          ensureInitialTerminal: true,
          shouldRecordHistory: true,
        ),
        throwsStateError,
      );

      expect(events, <String>['activate', 'hydrate']);
    },
  );

  test('stale hydration skips tabs, layout, and history', () async {
    final events = <String>[];
    var current = true;
    final coordinator = WorkbenchWorkspaceSelectionCoordinator(
      activateSelection: () => events.add('activate'),
      hydrate: ({required workspaceId, required ensureInitialTerminal}) async {
        events.add('hydrate');
        current = false;
        return WorkbenchWorkspaceSelectionHydration(
          tabs: <WorkspaceTabRecord>[_tab('tab')],
          layout: _layout(<String>['tab']),
        );
      },
      applyTabs: (_) => events.add('tabs'),
      applyLayout: (_) async => events.add('layout'),
      isSelectionCurrent: () => current,
      recordHistory: () {
        events.add('history');
        return true;
      },
      notifyHistoryChanged: () => events.add('notify'),
    );

    await coordinator.select(
      workspaceId: 'workspace',
      ensureInitialTerminal: false,
      shouldRecordHistory: true,
    );

    expect(events, <String>['activate', 'hydrate']);
  });

  test('layout failure preserves applied tabs and skips history', () async {
    final events = <String>[];
    final tabs = <WorkspaceTabRecord>[_tab('tab')];
    final coordinator = WorkbenchWorkspaceSelectionCoordinator(
      activateSelection: () => events.add('activate'),
      hydrate: ({required workspaceId, required ensureInitialTerminal}) async {
        events.add('hydrate');
        return WorkbenchWorkspaceSelectionHydration(
          tabs: tabs,
          layout: _layout(<String>['tab']),
        );
      },
      applyTabs: (_) => events.add('tabs'),
      applyLayout: (_) async {
        events.add('layout');
        throw StateError('layout failed');
      },
      isSelectionCurrent: () => true,
      recordHistory: () {
        events.add('history');
        return true;
      },
      notifyHistoryChanged: () => events.add('notify'),
    );

    await expectLater(
      coordinator.select(
        workspaceId: 'workspace',
        ensureInitialTerminal: false,
        shouldRecordHistory: true,
      ),
      throwsStateError,
    );

    expect(events, <String>['activate', 'hydrate', 'tabs', 'layout']);
  });
}

WorkbenchWorkspaceSelectionCoordinator _coordinator({
  required List<String> events,
  required bool recordHistoryResult,
}) => WorkbenchWorkspaceSelectionCoordinator(
  activateSelection: () => events.add('activate'),
  hydrate: ({required workspaceId, required ensureInitialTerminal}) async {
    events.add('hydrate:$ensureInitialTerminal');
    return WorkbenchWorkspaceSelectionHydration(
      tabs: <WorkspaceTabRecord>[_tab('tab')],
      layout: _layout(<String>['tab']),
    );
  },
  applyTabs: (_) => events.add('tabs'),
  applyLayout: (_) async => events.add('layout'),
  isSelectionCurrent: () => true,
  recordHistory: () {
    events.add('history');
    return recordHistoryResult;
  },
  notifyHistoryChanged: () => events.add('notify'),
);

WorkspaceTabRecord _tab(String id) => WorkspaceTabRecord(
  id: id,
  workspaceId: 'workspace',
  title: id,
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
);

WorkbenchLayout _layout(List<String> tabIds) =>
    WorkbenchLayout.single(workspaceId: 'workspace', tabIds: tabIds);
