import 'package:alera/src/features/workbench/application/workbench_persisted_tab_restore_plan.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('refreshes an already-open persisted tab without duplicating it', () {
    final stale = _tab('tab-1', title: 'Old');
    final restored = _tab('tab-1', title: 'Restored');
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[stale.id],
    );

    final plan = planWorkbenchPersistedTabRestore(
      currentTabs: <WorkspaceTabRecord>[stale],
      layout: layout,
      restoredTab: restored,
    );

    expect(plan.tabs, hasLength(1));
    expect(plan.tabs.single.title, 'Restored');
    expect(plan.layoutToPersist, isNull);
  });

  test(
    'adds a missing persisted tab to the active group and persists layout',
    () {
      final existing = _tab('tab-1');
      final restored = _tab('tab-2');
      final layout = WorkbenchLayout.single(
        workspaceId: 'workspace',
        tabIds: <String>[existing.id],
      );

      final plan = planWorkbenchPersistedTabRestore(
        currentTabs: <WorkspaceTabRecord>[existing],
        layout: layout,
        restoredTab: restored,
      );

      expect(plan.tabs.map((tab) => tab.id), <String>['tab-1', 'tab-2']);
      expect(plan.layoutToPersist, isNotNull);
      expect(plan.layoutToPersist!.groups.values.single.tabIds, <String>[
        'tab-1',
        'tab-2',
      ]);
    },
  );

  test('repairs a tab missing from layout even when the tab record is already local', () {
    final first = _tab('tab-1');
    final restored = _tab('tab-2', title: 'Restored');
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[first.id],
    );

    final plan = planWorkbenchPersistedTabRestore(
      currentTabs: <WorkspaceTabRecord>[
        first,
        _tab('tab-2', title: 'Old'),
      ],
      layout: layout,
      restoredTab: restored,
    );

    expect(plan.tabs.map((tab) => tab.title), <String>['tab-1', 'Restored']);
    expect(plan.layoutToPersist, isNotNull);
    expect(plan.layoutToPersist!.groupIdForTab('tab-2'), layout.activeGroupId);
  });

  test('sanitizes stale layout ids while restoring the persisted tab', () {
    final restored = _tab('tab-2');
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>['stale-tab'],
    );

    final plan = planWorkbenchPersistedTabRestore(
      currentTabs: const <WorkspaceTabRecord>[],
      layout: layout,
      restoredTab: restored,
    );

    expect(plan.layoutToPersist, isNotNull);
    expect(plan.layoutToPersist!.groups.values.single.tabIds, <String>[
      'tab-2',
    ]);
  });
}

WorkspaceTabRecord _tab(String id, {String? title}) {
  final now = DateTime.utc(2026, 9, 11);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: 'workspace',
    title: title ?? id,
    createdAt: now,
    updatedAt: now,
  );
}
