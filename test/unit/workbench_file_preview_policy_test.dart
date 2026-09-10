import 'package:alera/src/features/workbench/application/workbench_file_preview_policy.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('prefers the active preview slot in the group', () {
    final now = DateTime.utc(2026, 9, 10);
    final first = _tab('first', now, preview: true);
    final active = _tab('active', now, preview: true);
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[first.id, active.id],
    );

    final result = workbenchPreviewTabIdInGroup(
      layout: layout,
      tabs: <WorkspaceTabRecord>[first, active],
      groupId: layout.activeGroupId,
    );

    expect(result, active.id);
  });

  test('falls back to the first preview slot in group order', () {
    final now = DateTime.utc(2026, 9, 10);
    final first = _tab('first', now, preview: true);
    final activePermanent = _tab('permanent', now, preview: false);
    final second = _tab('second', now, preview: true);
    final baseLayout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[first.id, activePermanent.id, second.id],
    );
    final layout = baseLayout.setActiveTab(
      groupId: baseLayout.activeGroupId,
      tabId: activePermanent.id,
    );

    final result = workbenchPreviewTabIdInGroup(
      layout: layout,
      tabs: <WorkspaceTabRecord>[second, activePermanent, first],
      groupId: layout.activeGroupId,
    );

    expect(result, first.id);
  });

  test('returns null for missing group or group without a preview slot', () {
    final now = DateTime.utc(2026, 9, 10);
    final tab = _tab('permanent', now, preview: false);
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[tab.id],
    );

    expect(
      workbenchPreviewTabIdInGroup(
        layout: layout,
        tabs: <WorkspaceTabRecord>[tab],
        groupId: 'missing',
      ),
      isNull,
    );
    expect(
      workbenchPreviewTabIdInGroup(
        layout: layout,
        tabs: <WorkspaceTabRecord>[tab],
        groupId: layout.activeGroupId,
      ),
      isNull,
    );
  });
}

WorkspaceTabRecord _tab(String id, DateTime now, {required bool preview}) =>
    WorkspaceTabRecord(
      id: id,
      workspaceId: 'workspace',
      title: id,
      kind: WorkspaceTabKind.editor,
      createdAt: now,
      updatedAt: now,
      payload: <String, Object?>{
        workspaceTabFilePathPayloadKey: 'lib/$id.dart',
        if (preview) workspaceTabPreviewPayloadKey: true,
      },
    );
