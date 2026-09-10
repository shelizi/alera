import 'package:alera/src/features/workbench/application/workbench_file_tab_path_move_plan.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'updated records replace only matching current tabs without layout write',
    () {
      final now = DateTime.utc(2026, 9, 10);
      final first = _tab('first', now, path: 'docs/a.md');
      final second = _tab('second', now, path: 'docs/b.md');
      final updatedFirst = _tab(
        first.id,
        now.add(const Duration(minutes: 1)),
        path: 'docs/moved/a.md',
      );
      final unknown = _tab('unknown', now, path: 'docs/unknown.md');
      final layout = WorkbenchLayout.single(
        workspaceId: 'workspace',
        tabIds: <String>[first.id, second.id],
      );

      final plan = planWorkbenchFileTabPathMove(
        workspaceId: 'workspace',
        currentTabs: <WorkspaceTabRecord>[first, second],
        currentLayout: layout,
        updatedTabs: <WorkspaceTabRecord>[updatedFirst, unknown],
        closedTabIds: const <String>[],
      );

      expect(plan.tabs, <WorkspaceTabRecord>[updatedFirst, second]);
      expect(plan.layoutToPersist, isNull);
    },
  );

  test('closed tabs are removed and remaining layout is sanitized', () {
    final now = DateTime.utc(2026, 9, 10);
    final keep = _tab('keep', now, path: 'docs/keep.md');
    final closed = _tab('closed', now, path: 'docs/closed.md');
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[keep.id, closed.id],
    );

    final plan = planWorkbenchFileTabPathMove(
      workspaceId: 'workspace',
      currentTabs: <WorkspaceTabRecord>[keep, closed],
      currentLayout: layout,
      updatedTabs: const <WorkspaceTabRecord>[],
      closedTabIds: <String>[closed.id],
    );

    expect(plan.tabs, <WorkspaceTabRecord>[keep]);
    expect(plan.layoutToPersist, isNotNull);
    expect(plan.layoutToPersist!.groupIdForTab(closed.id), isNull);
    expect(plan.layoutToPersist!.groupIdForTab(keep.id), isNotNull);
    expect(plan.layoutToPersist!.activeTabId, keep.id);
  });

  test('closing every tab produces an empty single layout', () {
    final now = DateTime.utc(2026, 9, 10);
    final only = _tab('only', now, path: 'docs/only.md');
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[only.id],
    );

    final plan = planWorkbenchFileTabPathMove(
      workspaceId: 'workspace',
      currentTabs: <WorkspaceTabRecord>[only],
      currentLayout: layout,
      updatedTabs: const <WorkspaceTabRecord>[],
      closedTabIds: <String>[only.id],
    );

    expect(plan.tabs, isEmpty);
    expect(plan.layoutToPersist, isNotNull);
    expect(plan.layoutToPersist!.workspaceId, 'workspace');
    expect(plan.layoutToPersist!.groups.values.single.tabIds, isEmpty);
    expect(plan.layoutToPersist!.activeTabId, isNull);
  });

  test('missing layout falls back to a layout built from remaining tabs', () {
    final now = DateTime.utc(2026, 9, 10);
    final keep = _tab('keep', now, path: 'docs/keep.md');

    final plan = planWorkbenchFileTabPathMove(
      workspaceId: 'workspace',
      currentTabs: <WorkspaceTabRecord>[keep],
      currentLayout: null,
      updatedTabs: const <WorkspaceTabRecord>[],
      closedTabIds: const <String>['already-gone'],
    );

    expect(plan.tabs, <WorkspaceTabRecord>[keep]);
    expect(plan.layoutToPersist?.groupIdForTab(keep.id), isNotNull);
    expect(plan.layoutToPersist?.activeTabId, keep.id);
  });
}

WorkspaceTabRecord _tab(String id, DateTime now, {required String path}) =>
    WorkspaceTabRecord(
      id: id,
      workspaceId: 'workspace',
      title: id,
      kind: WorkspaceTabKind.editor,
      createdAt: now,
      updatedAt: now,
      payload: <String, Object?>{workspaceTabFilePathPayloadKey: path},
    );
