import 'package:alera/src/features/workbench/application/workbench_file_tab_open_plan.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('new tab appends to tabs and is added to the requested group', () {
    final now = DateTime.utc(2026, 9, 10);
    final mainTab = _fileTab('main', now, path: 'lib/main.dart');
    final sideTab = _fileTab('side', now, path: 'lib/side.dart');
    final base = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[mainTab.id],
    );
    final layout = base.splitWithGroup(
      targetGroupId: base.activeGroupId,
      zone: WorkbenchDropZone.right,
      newGroup: WorkbenchPaneGroup(
        id: 'side-group',
        tabIds: <String>[sideTab.id],
        activeTabId: sideTab.id,
      ),
    );
    final opened = _fileTab('opened', now, path: 'lib/opened.dart');

    final plan = planWorkbenchOpenedFileTab(
      previousTabs: <WorkspaceTabRecord>[mainTab, sideTab],
      layout: layout,
      targetGroupId: 'side-group',
      tab: opened,
    );

    expect(plan.tabs, <WorkspaceTabRecord>[mainTab, sideTab, opened]);
    expect(plan.layout.groupIdForTab(opened.id), 'side-group');
    expect(plan.layout.activeGroupId, 'side-group');
    expect(plan.layout.groups['side-group']?.activeTabId, opened.id);
    expect(plan.shouldForgetEditorSession, isFalse);
  });

  test('existing tab updates in place and stays in its existing group', () {
    final now = DateTime.utc(2026, 9, 10);
    final existing = _fileTab('existing', now, path: 'lib/a.dart');
    final sideTab = _fileTab('side', now, path: 'lib/side.dart');
    final base = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[existing.id],
    );
    final layout = base.splitWithGroup(
      targetGroupId: base.activeGroupId,
      zone: WorkbenchDropZone.right,
      newGroup: WorkbenchPaneGroup(
        id: 'side-group',
        tabIds: <String>[sideTab.id],
        activeTabId: sideTab.id,
      ),
    );
    final updated = _fileTab(
      existing.id,
      now.add(const Duration(minutes: 1)),
      path: existing.filePath!,
      title: 'renamed',
    );

    final plan = planWorkbenchOpenedFileTab(
      previousTabs: <WorkspaceTabRecord>[existing, sideTab],
      layout: layout,
      targetGroupId: 'side-group',
      tab: updated,
    );

    expect(plan.tabs, <WorkspaceTabRecord>[updated, sideTab]);
    expect(plan.layout.groupIdForTab(updated.id), base.activeGroupId);
    expect(plan.layout.activeGroupId, base.activeGroupId);
    expect(plan.layout.groups[base.activeGroupId]?.activeTabId, updated.id);
    expect(plan.shouldForgetEditorSession, isFalse);
  });

  test(
    'existing tab with changed backing identity requests editor-session forget',
    () {
      final now = DateTime.utc(2026, 9, 10);
      final existing = _fileTab('preview', now, path: 'lib/a.dart');
      final layout = WorkbenchLayout.single(
        workspaceId: 'workspace',
        tabIds: <String>[existing.id],
      );
      final retargeted = _fileTab(
        existing.id,
        now.add(const Duration(minutes: 1)),
        path: 'lib/b.dart',
      );

      final plan = planWorkbenchOpenedFileTab(
        previousTabs: <WorkspaceTabRecord>[existing],
        layout: layout,
        targetGroupId: layout.activeGroupId,
        tab: retargeted,
      );

      expect(plan.tabs, <WorkspaceTabRecord>[retargeted]);
      expect(plan.shouldForgetEditorSession, isTrue);
    },
  );

  test('kind change also counts as a changed backing identity', () {
    final now = DateTime.utc(2026, 9, 10);
    final existing = _fileTab('preview', now, path: 'docs/readme.md');
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[existing.id],
    );
    final viewer = _fileTab(
      existing.id,
      now.add(const Duration(minutes: 1)),
      path: existing.filePath!,
      kind: WorkspaceTabKind.markdownViewer,
    );

    final plan = planWorkbenchOpenedFileTab(
      previousTabs: <WorkspaceTabRecord>[existing],
      layout: layout,
      targetGroupId: layout.activeGroupId,
      tab: viewer,
    );

    expect(plan.shouldForgetEditorSession, isTrue);
  });
}

WorkspaceTabRecord _fileTab(
  String id,
  DateTime now, {
  required String path,
  String? title,
  WorkspaceTabKind kind = WorkspaceTabKind.editor,
}) => WorkspaceTabRecord(
  id: id,
  workspaceId: 'workspace',
  title: title ?? id,
  kind: kind,
  createdAt: now,
  updatedAt: now,
  payload: <String, Object?>{workspaceTabFilePathPayloadKey: path},
);
