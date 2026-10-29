import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

/// Immutable drag identity shared by the workbench and sidebar. A transfer
/// always re-parents the existing tab; never create a replacement PTY here.
class WorkbenchTabDragData {
  const WorkbenchTabDragData({
    required this.workspaceId,
    required this.sourceGroupId,
    required this.tabId,
    required this.kind,
    required this.sourceHostId,
  });

  final String workspaceId;
  final String sourceGroupId;
  final String tabId;
  final WorkspaceTabKind kind;
  final String sourceHostId;

  bool canTransferTo({
    required String targetWorkspaceId,
    required String targetHostId,
    required bool targetArchived,
  }) =>
      kind == WorkspaceTabKind.terminal &&
      workspaceId != targetWorkspaceId &&
      sourceHostId == targetHostId &&
      !targetArchived;
}
