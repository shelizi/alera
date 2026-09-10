import 'package:alera/src/features/workbench/application/workbench_file_preview_policy.dart';
import 'package:alera/src/features/workbench/application/workbench_file_tab_open_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_replaceable_tab_editor_sessions.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchReplaceableTabFactory = Future<WorkspaceTabRecord> Function({
  required String workspaceId,
  required bool preview,
  String? replacePreviewTabId,
});

typedef WorkbenchKeepPreviewTab = Future<WorkspaceTabRecord> Function(
  String tabId,
);

typedef WorkbenchPreviewPinned = List<WorkspaceTabRecord> Function(
  WorkspaceTabRecord tab,
);

final class WorkbenchReplaceableTabOpenResult {
  const WorkbenchReplaceableTabOpenResult({
    required this.tab,
    required this.plan,
  });

  final WorkspaceTabRecord tab;
  final WorkbenchOpenedFileTabPlan plan;
}

final class WorkbenchReplaceableTabOpenCoordinator {
  const WorkbenchReplaceableTabOpenCoordinator({
    required WorkbenchReplaceableTabEditorSessions editorSessions,
  }) : _editorSessions = editorSessions;

  final WorkbenchReplaceableTabEditorSessions _editorSessions;

  Future<WorkbenchReplaceableTabOpenResult> open({
    required String workspaceId,
    required List<WorkspaceTabRecord> previousTabs,
    required WorkbenchLayout layout,
    required String targetGroupId,
    required bool preview,
    required WorkbenchKeepPreviewTab keepPreviewTab,
    required WorkbenchPreviewPinned onPreviewPinned,
    required WorkbenchReplaceableTabFactory createTab,
  }) async {
    var tabsBeforeOpen = previousTabs;
    var replacePreviewTabId = preview
        ? workbenchPreviewTabIdInGroup(
            layout: layout,
            tabs: tabsBeforeOpen,
            groupId: targetGroupId,
          )
        : null;

    if (replacePreviewTabId != null &&
        _editorSessions.isDirty(replacePreviewTabId)) {
      final pinnedTab = await keepPreviewTab(replacePreviewTabId);
      tabsBeforeOpen = onPreviewPinned(pinnedTab);
      replacePreviewTabId = null;
    }

    final tab = await createTab(
      workspaceId: workspaceId,
      preview: preview,
      replacePreviewTabId: replacePreviewTabId,
    );
    final plan = planWorkbenchOpenedFileTab(
      previousTabs: tabsBeforeOpen,
      layout: layout,
      targetGroupId: targetGroupId,
      tab: tab,
    );
    if (plan.shouldForgetEditorSession) {
      _editorSessions.forget(tab.id);
    }
    return WorkbenchReplaceableTabOpenResult(tab: tab, plan: plan);
  }
}
