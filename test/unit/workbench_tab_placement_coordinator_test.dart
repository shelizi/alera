import 'package:alera/src/features/workbench/application/workbench_tab_placement_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_placement_plan.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('runs open, plan, tabs, layout, and after-placement in order', () async {
    final events = <String>[];
    final tab = _tab('new');
    final placement = WorkbenchTabPlacementPlan(
      tabs: <WorkspaceTabRecord>[tab],
      layout: _layout(<String>[tab.id]),
    );
    final coordinator = WorkbenchTabPlacementCoordinator(
      openTab: () async {
        events.add('open');
        return tab;
      },
      planPlacement: (opened) {
        events.add('plan:${opened.id}');
        return placement;
      },
      applyTabs: (tabs) => events.add('tabs:${tabs.single.id}'),
      applyLayout: (layout) async => events.add('layout:${layout.activeTabId}'),
      afterPlaced: (opened) => events.add('after:${opened.id}'),
    );

    final returned = await coordinator.run();

    expect(returned, same(tab));
    expect(events, <String>[
      'open',
      'plan:new',
      'tabs:new',
      'layout:new',
      'after:new',
    ]);
  });

  test('open failure stops every later step and propagates', () async {
    final events = <String>[];
    final coordinator = WorkbenchTabPlacementCoordinator(
      openTab: () async {
        events.add('open');
        throw StateError('open failed');
      },
      planPlacement: (_) {
        events.add('plan');
        throw StateError('should not run');
      },
      applyTabs: (_) => events.add('tabs'),
      applyLayout: (_) async => events.add('layout'),
    );

    await expectLater(coordinator.run(), throwsA(isA<StateError>()));
    expect(events, <String>['open']);
  });

  test(
    'planning failure stops before state application and propagates',
    () async {
      final events = <String>[];
      final coordinator = WorkbenchTabPlacementCoordinator(
        openTab: () async => _tab('new'),
        planPlacement: (_) {
          events.add('plan');
          throw StateError('plan failed');
        },
        applyTabs: (_) => events.add('tabs'),
        applyLayout: (_) async => events.add('layout'),
        afterPlaced: (_) => events.add('after'),
      );

      await expectLater(coordinator.run(), throwsA(isA<StateError>()));
      expect(events, <String>['plan']);
    },
  );

  test(
    'layout failure happens after tabs apply and prevents after-placement',
    () async {
      final events = <String>[];
      final tab = _tab('new');
      final coordinator = WorkbenchTabPlacementCoordinator(
        openTab: () async => tab,
        planPlacement: (_) => WorkbenchTabPlacementPlan(
          tabs: <WorkspaceTabRecord>[tab],
          layout: _layout(<String>[tab.id]),
        ),
        applyTabs: (_) => events.add('tabs'),
        applyLayout: (_) async {
          events.add('layout');
          throw StateError('layout failed');
        },
        afterPlaced: (_) => events.add('after'),
      );

      await expectLater(coordinator.run(), throwsA(isA<StateError>()));
      expect(events, <String>['tabs', 'layout']);
    },
  );
}

WorkspaceTabRecord _tab(String id) => WorkspaceTabRecord(
  id: id,
  workspaceId: 'workspace',
  title: id,
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
);

WorkbenchLayout _layout(List<String> tabIds) =>
    WorkbenchLayout.single(workspaceId: 'workspace', tabIds: tabIds);
